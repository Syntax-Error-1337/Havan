import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useQuery } from '@tanstack/react-query';

import { request } from '../../shared/api/client';
import { radio, type NearbyDevice } from '../../shared/lib/ble';

/** A device that stops advertising is gone; keep it on screen a moment first. */
const FORGET_AFTER_MS = 9_000;

export type RadarState = 'idle' | 'scanning' | 'denied' | 'unsupported';

export type PublicProduct = {
  product_key: string;
  name: string;
  category: string;
  icon: string | null;
  hash: string;
};

/**
 * The scan, as a hook.
 *
 * Advertisements arrive many times a second per device, so they are folded by
 * product hash + serial tail and kept in a ref — re-rendering a list on every
 * packet would make the radar jitter and the phone hot.
 */
export function useRadar(active: boolean) {
  const [state, setState] = useState<RadarState>('idle');
  const [devices, setDevices] = useState<NearbyDevice[]>([]);
  const seen = useRef(new Map<string, { device: NearbyDevice; at: number }>());

  const flush = useCallback(() => {
    const now = Date.now();
    const alive: NearbyDevice[] = [];
    for (const [key, entry] of seen.current) {
      if (now - entry.at > FORGET_AFTER_MS) {
        seen.current.delete(key);
        continue;
      }
      alive.push(entry.device);
    }
    alive.sort((a, b) => b.rssi - a.rssi);
    setDevices(alive);
  }, []);

  useEffect(() => {
    if (!active) {
      setState('idle');
      return;
    }

    let stop: (() => void) | undefined;
    let sweep: ReturnType<typeof setInterval> | undefined;
    let cancelled = false;

    void (async () => {
      const ok = await radio.available();
      if (cancelled) return;
      if (!ok) {
        setState('unsupported');
        return;
      }

      setState('scanning');
      stop = radio.scan((device) => {
        const key = `${device.productHash}:${device.serialTail}`;
        seen.current.set(key, { device, at: Date.now() });
        flush();
      });
      sweep = setInterval(flush, 1_500);
    })();

    return () => {
      cancelled = true;
      stop?.();
      if (sweep) clearInterval(sweep);
      seen.current.clear();
      setDevices([]);
    };
  }, [active, flush]);

  // One lookup for everything on the radar, keyed by the hashes actually seen.
  const hashes = useMemo(
    () => [...new Set(devices.map((d) => d.productHash))].sort().join(','),
    [devices],
  );

  const catalog = useQuery({
    queryKey: ['catalog', 'products', hashes],
    queryFn: () =>
      request<PublicProduct[]>(`/catalog/products?hashes=${encodeURIComponent(hashes)}`),
    enabled: hashes.length > 0,
    staleTime: 5 * 60_000,
  });

  const known = useMemo(() => {
    const byHash = new Map((catalog.data ?? []).map((p) => [p.hash, p]));
    return devices.map((device) => {
      const product = byHash.get(device.productHash);
      return {
        ...device,
        // An unknown hash is a real state: it is either a product we have not
        // published yet or someone else's device. Say so rather than hide it.
        name: product?.name,
        category: product?.category,
      };
    });
  }, [devices, catalog.data]);

  return { state, devices: known, loading: catalog.isLoading };
}
