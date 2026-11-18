import { useEffect, useState } from 'react';

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
  // The box is local and the filter is debounced: every keystroke is not a request, and a pasted serial
  // is one.
  const [typed, setTyped] = useState('');
  useEffect(() => {
    const timer = setTimeout(() => {
      const q = typed.trim();
      setFilter((current) => (current.q === (q || undefined) ? current : { ...current, q: q || undefined }));
    }, 250);
    return () => clearTimeout(timer);
  }, [typed]);
  const [loadingAll, setLoadingAll] = useState(false);
  const { data, isPending, error, fetchNextPage, hasNextPage, isFetchingNextPage, refetch } =
    useFleet(filter);
  // The count the page cannot derive from what it has loaded: "650 devices shown · more available" told a
  // vendor nothing about whether the rest was five devices or five thousand.
  const { data: stats } = useFleetStats();

  const devices = data?.pages.flatMap((p) => p.devices) ?? [];

  /**
   * Change the order, and start again from the first page.
   *
   * The cursor is a position in one particular order — page two of "serial A–Z" is nothing like page two of "newest
   * first" — so a sort change has to reset the list rather than page into the old one.
   */
  const sortBy = (column: 'serial' | 'name' | 'last_seen') => {
    const current = filter.sort ?? 'created';
    const descending = filter.dir ? filter.dir === 'desc' : current === 'created' || current === 'last_seen';
    const nextDescending = current === column ? !descending : column === 'last_seen';
    setFilter({
      ...filter,
      sort: column,
      dir: nextDescending ? 'desc' : 'asc',
    });
  };
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
      {/* The controls wrap rather than widen the page: a 220 px search box beside a select beside an
          <h1> is wider than a 420 px phone, and the layout suite measured exactly that — a sideways
          scrollbar on a screen that had none before the search box arrived. */}
      <div
        className="row"
        style={{ justifyContent: 'space-between', alignItems: 'baseline', flexWrap: 'wrap', gap: 8 }}
      >
        <h1>Fleet</h1>
        <div className="row" style={{ gap: 8, alignItems: 'center', flexWrap: 'wrap' }}>
          {/* The first thing anyone does with a fleet is find one device: a serial from a customer's
              photo, or a name they remember. Without this, a vendor with 1 909 devices paged through a
              table to do it, and a vendor with a million could not do it at all. */}
          <input
            className="input"
            type="search"
            aria-label="Search serial or name"
            placeholder="Serial or name"
            data-testid="fleet-search"
            value={typed}
            onChange={(e) => setTyped(e.target.value)}
            style={{ minWidth: 0, flex: '1 1 160px', maxWidth: 260 }}
          />
          {/* "State" over "All devices" was two vocabularies for one control, and the column beside it says
              "Status". One word, and the values are statuses rather than devices. */}
          <span className="field-label">Status</span>
          <select
            aria-label="Filter by status"
            value={filter.state ?? ''}
            onChange={(e) =>
              setFilter({ ...filter, state: (e.target.value || undefined) as FleetFilter['state'] })
            }
          >
            <option value="">All statuses</option>
            <option value="online">Online</option>
            <option value="offline">Offline</option>
            <option value="bound">Bound to a customer</option>
            <option value="unbound">In stock (unbound)</option>
          </select>
        </div>
      </div>

      {/* The count and the controls sit above the table, not below it: at 1 900 devices the buttons were
          two hundred rows away, which on a phone is the same as not having them. */}
      <div className="row" style={{ gap: 8, alignItems: 'center', flexWrap: 'wrap', marginBottom: 12 }}>
        {/* "shown" was read as "on screen", and at 1,900 devices eleven rows are visible while two hundred are
            loaded. The word says which of those it is. */}
        <span className="muted" data-testid="fleet-count">
          {devices.length.toLocaleString()} of {total.toLocaleString()} device{total === 1 ? '' : 's'}
          {filter.q ? ` matching “${filter.q}”` : ''} loaded
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
              {/* "Load more" did not say how many; the page size is 200 and the button can say so. */}
              {isFetchingNextPage && !loadingAll ? 'Loading…' : 'Load 200 more'}
            </button>
            <button
              className="btn btn--quiet"
              onClick={() => void loadAll()}
              disabled={isFetchingNextPage || loadingAll}
            >
              {/* "Load all 1,909" beside "200 loaded" reads as if two hundred were missing from the total. The
                  number is what is left to fetch, which is the thing the press does. */}
              {loadingAll
                ? `Loading ${devices.length.toLocaleString()} of ${total.toLocaleString()}…`
                : `Load the other ${(total - devices.length).toLocaleString()}`}
            </button>
          </>
        ) : null}
      </div>

      {devices.length === 0 ? (
        <div className="card">
          {filter.q || filter.state ? (
            <>
              <p className="muted">
                No device matches{filter.q ? ` “${filter.q}”` : ''}
                {filter.state ? ` with status ${filter.state}` : ''}.
              </p>
              <button
                className="btn btn--quiet"
                onClick={() => {
                  setTyped('');
                  setFilter({});
                }}
              >
                Clear the filters
              </button>
            </>
          ) : (
            <p className="muted">
              No devices yet. Devices appear here as soon as a customer pairs one, or
              as soon as manufacturing claims a serial.
            </p>
          )}
        </div>
      ) : (
        <TableFrame label="Fleet devices">
  <table className="table">
            <thead>
              <tr>
                <SortableHeader
                  label="Serial"
                  column="serial"
                  filter={filter}
                  onSort={sortBy}
                />
                <th>Product</th>
                <SortableHeader label="Name" column="name" filter={filter} onSort={sortBy} />
                <th>Status</th>
                <th>Firmware</th>
                <SortableHeader
                  label="Last seen"
                  column="last_seen"
                  filter={filter}
                  onSort={sortBy}
                  className="nowrap"
                />
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
                    {/* A second fact, given a shape of its own. As plain text it read as one phrase with the pill —
                        "offline bound" — which is not a status anybody has. */}
                    <span
                      className="badge badge--quiet"
                      title={device.bound ? 'Bound to a customer' : 'In stock: manufactured, never claimed'}
                    >
                      {device.bound ? 'bound' : 'unbound'}
                    </span>
                  </td>
                  {/* A dash that says what it means. "—" in a column of dashes is either missing data or a broken
                      query, and a support engineer should not have to guess which. */}
                  <td className="mono" title={device.fw_version ? undefined : 'No firmware has been reported for this serial'}>
                    {device.fw_version ?? '—'}
                  </td>
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

/**
 * A column header that is also the control.
 *
 * `aria-sort` is the part that matters: a screen reader announces "sorted ascending" from the header, which is the only
 * way a sortable table is usable without sight. The arrow is drawn with a character rather than an icon font because one
 * glyph is not worth a dependency, and it is `aria-hidden` so it is not read out as "black up-pointing triangle".
 */
function SortableHeader({
  label,
  column,
  filter,
  onSort,
  className,
}: {
  label: string;
  column: 'serial' | 'name' | 'last_seen';
  filter: FleetFilter;
  onSort: (column: 'serial' | 'name' | 'last_seen') => void;
  className?: string;
}) {
  const active = filter.sort === column;
  const descending = active
    ? filter.dir === 'desc'
    : (filter.sort ?? 'created') === 'created' || (filter.sort ?? 'created') === 'last_seen';

  return (
    <th
      className={className}
      aria-sort={active ? (descending ? 'descending' : 'ascending') : 'none'}
    >
      <button
        type="button"
        className="th-sort"
        onClick={() => onSort(column)}
        aria-label={`Sort by ${label.toLowerCase()}${active ? (descending ? ', descending' : ', ascending') : ''}`}
      >
        {label}
        {/* Drawn by CSS from `aria-sort`, not text: a header that copies out as "Serial↕" is a header whose glyph
            leaked into the table, and the arrow depends on state rather than on the column. */}
      </button>
    </th>
  );
}
