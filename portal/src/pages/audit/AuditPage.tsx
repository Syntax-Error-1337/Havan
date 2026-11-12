import { useAudit, type AuditEntry } from '../../entities/audit';

/**
 * The audit trail.
 *
 * An append-only list of the actions that change something a vendor might have to
 * explain: publishing a product, minting or revoking a key, starting or halting a
 * rollout. Read-only on purpose — a trail you can edit from the UI is not a trail.
 */
export function AuditPage() {
  const { data, isPending, error, fetchNextPage, hasNextPage, isFetchingNextPage, refetch } =
    useAudit();

  const entries = data?.pages.flatMap((p) => p.entries) ?? [];

  if (isPending) return <p className="muted">Reading the trail…</p>;
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
      <h1>Audit log</h1>
      <p className="muted">
        Newest first. Append-only: nothing here can be edited or deleted from the portal.
      </p>

      {entries.length === 0 ? (
        <div className="card">
          <p className="muted">
            Nothing recorded yet. Publishing a product, creating or revoking an API key and
            starting or halting a rollout all appear here.
          </p>
        </div>
      ) : (
        <table className="table">
          <thead>
            <tr>
              <th className="nowrap">When</th>
              <th>Action</th>
              <th>Target</th>
              <th>Who</th>
              <th className="nowrap">From</th>
            </tr>
          </thead>
          <tbody>
            {entries.map((entry) => (
              <tr key={entry.id}>
                <td className="nowrap muted">{when(entry.ts)}</td>
                <td>
                  <span className="mono">{entry.action}</span>
                  <Detail detail={entry.detail} />
                </td>
                <td className="muted">
                  {entry.target_kind}
                  {entry.target_id ? (
                    <span className="mono"> {entry.target_id.slice(0, 8)}</span>
                  ) : null}
                </td>
                <td className="muted">{who(entry)}</td>
                <td className="mono muted">{entry.ip ?? '—'}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

      {hasNextPage ? (
        <button onClick={() => void fetchNextPage()} disabled={isFetchingNextPage}>
          {isFetchingNextPage ? 'Loading…' : 'Load more'}
        </button>
      ) : null}
    </div>
  );
}

/** "who" in words: a person, a vendor's server, or a job nobody pressed a button for. */
export function who(entry: AuditEntry): string {
  switch (entry.actor_kind) {
    case 'user':
      return entry.actor_id ? `user ${entry.actor_id.slice(0, 8)}` : 'a user';
    case 'api_key':
      return 'an API key';
    case 'system':
      return 'automatic';
  }
}

/** The details that fit on one line, as `key=value` pairs. */
function Detail({ detail }: { detail: Record<string, unknown> }) {
  const pairs = Object.entries(detail ?? {}).filter(
    ([, v]) => v !== null && v !== undefined && typeof v !== 'object',
  );
  if (pairs.length === 0) return null;
  return (
    <div className="muted" style={{ fontSize: 12 }}>
      {pairs.map(([k, v]) => `${k}=${String(v)}`).join(' · ')}
    </div>
  );
}

/** Local time, to the minute: an audit row is read to place an incident, not to the ms. */
export function when(iso: string): string {
  const d = new Date(iso);
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}
