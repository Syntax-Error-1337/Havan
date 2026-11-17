/**
 * Whether a device is *heard from*, as distinct from whether the platform believes it is online.
 *
 * A device that drops off Wi-Fi mid-session keeps its online flag until the broker's last will fires or
 * the presence sweep notices — which can be minutes, and after a partial network failure on a phone
 * hotspot has been seen to be hours. In that window the app happily shows yesterday's temperature as
 * this morning's, and the customer's conclusion is that the platform is lying rather than that the bulb
 * is asleep.
 *
 * So every screen that shows a device's state classifies it here, with one threshold, rather than each
 * deciding for itself what "recently" means.
 */

/** Long enough to cover a sleepy sensor's report interval; short enough to catch a real dropout. */
export const SILENT_AFTER_MS = 15 * 60_000;

export type Freshness = 'live' | 'stale' | 'offline';

type Seen = { online: boolean; last_seen_at?: string | null };

/** `offline` (the platform says so), `stale` (online but silent), or `live`. */
export function freshness(device: Seen, now: number = Date.now()): Freshness {
  if (!device.online) return 'offline';
  if (!device.last_seen_at) return 'live';
  const seen = new Date(device.last_seen_at).getTime();
  if (Number.isNaN(seen)) return 'live';
  return now - seen > SILENT_AFTER_MS ? 'stale' : 'live';
}

/**
 * How long it has been quiet, in words: "Silent for 4h".
 *
 * Returns `null` when there is nothing worth saying — a device heard from a minute ago is not news, and
 * a line that always says something is a line people stop reading.
 */
export function silenceWords(device: Seen, now: number = Date.now()): string | null {
  if (!device.last_seen_at) return null;
  const seen = new Date(device.last_seen_at).getTime();
  if (Number.isNaN(seen)) return null;

  const minutes = Math.round((now - seen) / 60_000);
  if (minutes < 2) return null;
  if (minutes < 60) return `Silent for ${minutes}m`;
  if (minutes < 1440) return `Silent for ${Math.round(minutes / 60)}h`;
  return `Silent for ${Math.round(minutes / 1440)}d`;
}
