import { useState } from 'react';

import { useFleet, useFleetStats, type FleetFilter } from '../../entities/fleet';
import { TableFrame } from '../../shared/ui/TableFrame';

/**
 * The fleet: every device the vendor has shipped, and what it is doing.
 *
 * Two things are deliberately absent — the customer's name and their home. A vendor
 * sells hardware; the person who bought it is not their user. What they do get is
 * enough to run support: is it bound, when was it last seen, what firmware is on it.
 */
export function FleetPage() {
  const [filter, setFilter] = useState<FleetFilter>({});
  const [loadingAll, setLoadingAll] = useState(false);
  const { data, isPending, error, fetchNextPage, hasNextPage, isFetchingNextPage, refetch } =
    useFleet(filter);
  // The count the page cannot derive from what it has loaded: "650 devices shown · more available" told a
  // vendor nothing about whether the rest was five devices or five thousand.
  const { data: stats } = useFleetStats();

  const devices = data?.pages.flatMap((p) => p.devices) ?? [];
  const total = stats?.devices.total ?? devices.length;

  /**
   * Everything, in one press.
   *
   * Ten requests for a 1 900-device fleet, walked one at a time so a slow page cannot start ten at once.
   * The cap is a guard against a server that never stops returning a cursor, not a product limit.
   */
  const loadAll = async () => {
    setLoadingAll(true);
    try {
      for (let page = 0; page < 40 && hasNextPage; page += 1) {
        await fetchNextPage();
        if (!hasNextPage) break;
      }
    } finally {
      setLoadingAll(false);
    }
  };

  if (isPending) return <p className="muted">Loading the fleet…</p>;
  if (error) {
    return (
      <div className="card">
        <p className="error">{error.message}</p>
        <button onClick={() => void refetch()}>Try again</button>
      </div>
    );
  }

  return (
    <div>
      <div className="row" style={{ justifyContent: 'space-between', alignItems: 'baseline' }}>
        <h1>Fleet</h1>
        <div className="row" style={{ gap: 8 }}>
          <span className="field-label">State</span>
          <select
            aria-label="Filter by state"
            value={filter.state ?? ''}
            onChange={(e) =>
              setFilter({ ...filter, state: (e.target.value || undefined) as FleetFilter['state'] })
            }
          >
            <option value="">All devices</option>
            <option value="online">Online</option>
            <option value="offline">Offline</option>
            <option value="bound">Bound to a customer</option>
            <option value="unbound">In stock / unbound</option>
          </select>
        </div>
      </div>

      {/* The count and the controls sit above the table, not below it: at 1 900 devices the buttons were
          two hundred rows away, which on a phone is the same as not having them. */}
      <div className="row" style={{ gap: 8, alignItems: 'center', flexWrap: 'wrap', marginBottom: 12 }}>
        <span className="muted">
          {devices.length.toLocaleString()} of {total.toLocaleString()} device{total === 1 ? '' : 's'} shown
        </span>
        {/* One set of controls, and it lives here rather than under the table: at 1 900 devices the bottom
            of the page is two hundred rows away, which on a phone is the same as not having the buttons. */}
        {hasNextPage ? (
          <>
            <button
              className="btn"
              onClick={() => void fetchNextPage()}
              disabled={isFetchingNextPage || loadingAll}
            >
              {isFetchingNextPage && !loadingAll ? 'Loading…' : 'Load more'}
            </button>
            <button
              className="btn btn--quiet"
              onClick={() => void loadAll()}
              disabled={isFetchingNextPage || loadingAll}
            >
              {loadingAll
                ? `Loading ${devices.length.toLocaleString()} of ${total.toLocaleString()}…`
                : `Load all ${total.toLocaleString()}`}
            </button>
          </>
        ) : null}
      </div>

      {devices.length === 0 ? (
        <div className="card">
          <p className="muted">
            No devices match. Devices appear here as soon as a customer pairs one, or
            as soon as manufacturing claims a serial.
          </p>
        </div>
      ) : (
        <TableFrame label="Fleet devices">
  <table className="table">
            <thead>
              <tr>
                <th>Serial</th>
                <th>Product</th>
                <th>Name</th>
                <th>Status</th>
                <th>Firmware</th>
                <th className="nowrap">Last seen</th>
              </tr>
            </thead>
            <tbody>
              {devices.map((device) => (
                <tr key={device.id}>
                  <td className="mono">{device.serial}</td>
                  <td>{device.product_name}</td>
                  <td>{device.name}</td>
                  <td>
                    <span
                      className={`badge ${device.online ? 'badge--online' : 'badge--offline'}`}
                      data-testid="fleet-state"
                    >
                      {device.online ? 'online' : 'offline'}
                    </span>{' '}
                    <span className="muted">{device.bound ? 'bound' : 'unbound'}</span>
                  </td>
                  <td className="mono">{device.fw_version ?? '—'}</td>
                  <td className="muted">{device.last_seen_at ? ago(device.last_seen_at) : 'never'}</td>
                </tr>
              ))}
            </tbody>
          </table>
</TableFrame>
      )}

    </div>
  );
}

/** A duration, not a timestamp: "when did it last talk to us" is the question. */
export function ago(iso: string): string {
  const seconds = Math.max(0, Math.round((Date.now() - new Date(iso).getTime()) / 1000));
  if (seconds < 60) return `${seconds}s ago`;
  if (seconds < 3_600) return `${Math.round(seconds / 60)}m ago`;
  if (seconds < 86_400) return `${Math.round(seconds / 3_600)}h ago`;
  return `${Math.round(seconds / 86_400)}d ago`;
}
