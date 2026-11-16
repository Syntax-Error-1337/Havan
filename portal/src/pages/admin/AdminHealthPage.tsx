import { useHealth, useOverview } from '../../entities/admin';

/**
 * The screen an on-call rotation opens first.
 *
 * Four questions, in the order they are asked: is anything talking to us, is anything stuck, is
 * anything being lost, and is anybody selling. Every figure comes from the platform's own tables —
 * no log scraping — so it can be trusted from a browser that is not on the box.
 */
export function AdminHealthPage() {
  const health = useHealth();
  const overview = useOverview();

  if (health.isPending) return <p className="muted">Reading the platform…</p>;
  if (health.error) return <p className="error">{health.error.message}</p>;

  const h = health.data;
  const talking = h.online > 0 && h.seen_recently > 0;
  const quiet = h.seen_recently === 0 && h.devices > 0;

  return (
    <>
      <h2 style={{ marginTop: 0 }}>Health</h2>
      <p className="muted" style={{ marginTop: -8 }}>
        Refreshed every 15 seconds. Counts only — a number that needs interpretation at 3am is a
        number nobody reads.
      </p>

      <div className="row" style={{ gap: 16, flexWrap: 'wrap', marginBottom: 24 }}>
        <Tile
          label="Devices"
          value={`${h.online} / ${h.devices}`}
          note="online / total"
          tone={talking ? 'ok' : quiet ? 'warn' : undefined}
        />
        <Tile
          label="Heard in 15 min"
          value={String(h.seen_recently)}
          note="reported recently"
          tone={h.seen_recently === 0 && h.devices > 0 ? 'warn' : undefined}
        />
        <Tile label="Points last hour" value={String(h.points_last_hour)} note="telemetry" />
        <Tile label="Commands last hour" value={String(h.commands_last_hour)} note="requested" />
      </div>

      <div className="row" style={{ gap: 16, flexWrap: 'wrap', marginBottom: 24 }}>
        <Tile
          label="Undelivered commands"
          value={String(h.undelivered)}
          note="refused while the broker was down, waiting"
          tone={h.undelivered > 0 ? 'warn' : 'ok'}
        />
        <Tile
          label="Unsent notifications"
          value={String(h.undelivered_notifications)}
          note="queued for a phone"
          tone={h.undelivered_notifications > 50 ? 'warn' : undefined}
        />
        <Tile
          label="Webhook backlog"
          value={String(h.webhook_backlog)}
          note="waiting on a vendor's endpoint"
          tone={h.webhook_backlog > 100 ? 'warn' : undefined}
        />
        {overview.data ? (
          <Tile
            label="Vendors"
            value={`${overview.data.vendors - overview.data.suspended} / ${overview.data.vendors}`}
            note="selling / total"
            tone={overview.data.suspended > 0 ? 'warn' : undefined}
          />
        ) : null}
      </div>

      {quiet ? (
        <p className="card" data-testid="health-quiet">
          <strong>Nothing has reported in fifteen minutes.</strong> Either the fleet is asleep or the
          broker link is down — check <code>/healthz</code> before paging anybody.
        </p>
      ) : null}

      <p className="muted">
        <strong>Online</strong> is what the platform believes from the last shadow report;{' '}
        <strong>heard recently</strong> is what actually arrived. When they disagree, a device is
        holding a stale connection — a real fault the first number alone hides.
      </p>
    </>
  );
}

function Tile({
  label,
  value,
  note,
  tone,
}: {
  label: string;
  value: string;
  note: string;
  tone?: 'ok' | 'warn';
}) {
  return (
    <div className="card" style={{ minWidth: 180, flex: '1 1 180px' }}>
      <div className="muted" style={{ fontSize: 12 }}>
        {label}
      </div>
      <div
        style={{
          fontSize: 26,
          fontWeight: 600,
          color:
            tone === 'warn'
              ? 'var(--danger, #b42318)'
              : tone === 'ok'
                ? 'var(--ok, #1f7a4d)'
                : undefined,
        }}
      >
        {value}
      </div>
      <div className="muted" style={{ fontSize: 12 }}>
        {note}
      </div>
    </div>
  );
}
