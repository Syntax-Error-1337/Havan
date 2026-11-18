import { useState } from 'react';

import { useAudit, type AuditEntry } from '../../entities/audit';
import { TableFrame } from '../../shared/ui/TableFrame';

/**
 * The audit trail.
 *
 * An append-only list of the actions that change something a vendor might have to
 * explain: publishing a product, minting or revoking a key, starting or halting a
 * rollout. Read-only on purpose — a trail you can edit from the UI is not a trail.
 */
export function AuditPage() {
  const [action, setAction] = useState('');
  const [actor, setActor] = useState('');
  // Trimmed on the way in: a filter of spaces is the same as no filter, and asking the server to scan for it is waste.
  const filters = { action: action.trim() || undefined, actor: actor.trim() || undefined };
  const narrowed = Boolean(filters.action || filters.actor);

  const { data, isPending, error, fetchNextPage, hasNextPage, isFetchingNextPage, refetch } =
    useAudit(filters);

  const entries = data?.pages.flatMap((p) => p.entries) ?? [];

  // Deliberately not an early return: a filter that unmounts the page while it loads takes the box the person is typing
  // in with it, so every keystroke ended in "Reading the trail…" and a lost caret.

  return (
    <div>
      <h1>Audit log</h1>
      <p className="muted">
        Newest first. Append-only: nothing here can be edited or deleted from the portal.
      </p>

      {/* Two filters, because the two questions are "what happened" and "who did it". An empty box is no filter: the
          server sees the parameter only when there is something in it. */}
      <div className="row" style={{ gap: 8, marginBottom: 16, flexWrap: 'wrap' }}>
        <label className="field" style={{ margin: 0 }}>
          <span>Action</span>
          <input
            list="audit-actions"
            value={action}
            onChange={(e) => setAction(e.target.value)}
            placeholder="api_key.revoked"
            aria-label="Filter by action"
            data-testid="audit-action"
          />
        </label>
        <datalist id="audit-actions">
          {/* Suggested from what the trail actually contains, not from a list in this file: a hardcoded set drifts the
              first time a new action is added to the platform, and the field would offer yesterday's vocabulary. */}
          {[...new Set(entries.map((entry) => entry.action))].sort().map((name) => (
            <option key={name} value={name} />
          ))}
        </datalist>
        <label className="field" style={{ margin: 0 }}>
          <span>Actor</span>
          <input
            value={actor}
            onChange={(e) => setActor(e.target.value)}
            placeholder="part of an email"
            aria-label="Filter by actor"
            data-testid="audit-actor"
          />
        </label>
        {narrowed ? (
          <button
            className="btn"
            onClick={() => {
              setAction('');
              setActor('');
            }}
          >
            Clear
          </button>
        ) : null}
      </div>

      {isPending ? <p className="muted">Reading the trail…</p> : null}

      {error ? (
        <div className="card">
          <p className="error">{error.message}</p>
          <button onClick={() => void refetch()}>Try again</button>
        </div>
      ) : null}

      {!isPending && !error && entries.length === 0 && narrowed ? (
        <div className="card">
          <p className="muted">
            Nothing matches {filters.action ? `the action ${filters.action}` : ''}
            {filters.action && filters.actor ? ' for ' : ''}
            {filters.actor ? `an actor containing “${filters.actor}”` : ''}. The trail is append-only — this is an
            empty answer, not a missing one.
          </p>
        </div>
      ) : null}

      {!isPending && !error && entries.length === 0 && !narrowed ? (
        <div className="card">
          <p className="muted">
            Nothing recorded yet. Publishing a product, creating or revoking an API key and
            starting or halting a rollout all appear here.
          </p>
        </div>
      ) : (
        <TableFrame label="Audit trail">
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
                    {/* The target column already names what was acted on; repeating it here as `url=…` is the same
                        fact twice on one row. */}
                    <Detail detail={entry.detail} already={targetLabel(entry)} />
                  </td>
                  {/* What was acted on, in the words a person would use. "vendor_webhook 01a1eba6" is a row in a
                      table, not an answer; the id stays in the title for when it is the right level of detail. */}
                  <td className="muted" title={entry.target_id ?? undefined}>
                    {targetLabel(entry)}
                  </td>
                  <td className="muted">{who(entry)}</td>
                  <td className="mono muted">{entry.ip ?? '—'}</td>
                </tr>
              ))}
            </tbody>
          </table>
</TableFrame>
      )}

      {hasNextPage ? (
        <button onClick={() => void fetchNextPage()} disabled={isFetchingNextPage}>
          {isFetchingNextPage ? 'Loading…' : 'Load more'}
        </button>
      ) : null}
    </div>
  );
}

/**
 * "who" in words: a person, a vendor's server, or a job nobody pressed a button for.
 *
 * The address first, because "who did this" is the question an audit trail exists to answer and nobody can answer it
 * from a uuid prefix. The id stays as the fallback for a person whose account has since been removed — which is itself
 * a fact worth seeing in a trail.
 */
export function who(entry: AuditEntry): string {
  switch (entry.actor_kind) {
    case 'user':
      if (entry.actor_email) return entry.actor_email;
      return entry.actor_id ? `user ${entry.actor_id.slice(0, 8)}` : 'a user';
    case 'api_key':
      return 'an API key';
    case 'system':
      return 'automatic';
  }
}

/**
 * What the action was done to, named rather than numbered.
 *
 * The audit row carries the resource's kind and id; the human label lives in the detail the action recorded. Where there
 * is none — an old row, or an action whose subject has no name — the kind and a short id are still better than nothing,
 * because at least they say which sort of thing changed.
 */
export function targetLabel(entry: AuditEntry): string {
  const detail = (entry.detail ?? {}) as Record<string, unknown>;
  const named = [detail.url, detail.name, detail.email, detail.serial].find(
    (value): value is string => typeof value === 'string' && value.length > 0,
  );
  if (named) return named;

  const kind = entry.target_kind.replace(/_/g, ' ');
  return entry.target_id ? `${kind} ${entry.target_id.slice(0, 8)}` : kind;
}

/** The details that fit on one line, as `key=value` pairs. */
function Detail({
  detail,
  already,
}: {
  detail: Record<string, unknown>;
  /** A value shown elsewhere in the row, so it is not printed twice. */
  already?: string;
}) {
  const pairs = Object.entries(detail ?? {}).filter(
    ([, v]) =>
      v !== null && v !== undefined && typeof v !== 'object' && String(v) !== already,
  );
  if (pairs.length === 0) return null;
  return (
    <div className="muted" style={{ fontSize: 12 }}>
      {pairs.map(([k, v]) => `${k}=${String(v)}`).join(' · ')}
    </div>
  );
}

/** Local time, to the minute: an audit row is read to place an incident, not to the ms. */
/**
 * To the second, in local time.
 *
 * Minutes were not enough to order a trail: four entries shared 06:35, so "newest first" could not be checked against
 * the screen. Seconds do not make it a proof — the row's own id and timestamp are in its title for that.
 */
export function when(iso: string): string {
  const d = new Date(iso);
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(
    d.getMinutes(),
  )}:${pad(d.getSeconds())}`;
}
