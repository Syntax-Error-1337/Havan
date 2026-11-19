import { useState } from 'react';

import { useAuditPage } from '../../entities/admin';
import { TableFrame } from '../../shared/ui/TableFrame';

function when(ts: string): string {
  const at = new Date(ts);
  return Number.isNaN(at.getTime()) ? ts : at.toISOString().replace('T', ' ').slice(0, 19);
}

/**
 * Every action anybody took, newest first.
 *
 * The record existed and only a vendor could read their own slice of it, so the operator could
 * suspend a vendor or certify a product with nothing on screen to say who did it or when. This is the
 * same page, widened — not a second log.
 */
export function AdminAuditPage() {
  const [before, setBefore] = useState<string | undefined>(undefined);
  const { data, isPending, error } = useAuditPage(before);

  return (
    <>
      <h2 style={{ marginTop: 0 }}>Audit</h2>
      <p className="muted" style={{ marginTop: -8 }}>
        Who did what, across every vendor. Entries are ordered by time and id together, so two actions
        in the same millisecond cannot hide one of them at a page boundary.
      </p>

      {error ? <p className="error">{error.message}</p> : null}
      {isPending ? <p className="muted">Loading audit…</p> : null}

      {data ? (
        <>
          <div className="row" style={{ marginBottom: 12 }}>
            {before ? (
              <button className="btn" onClick={() => setBefore(undefined)}>
                Newest
              </button>
            ) : null}
            <span className="muted" style={{ marginLeft: 'auto' }}>
              {data.entries.length} entries
            </span>
          </div>

          <TableFrame label="Audit log">
            <table className="table" data-testid="audit-table">
              <thead>
                <tr>
                  <th scope="col">When</th>
                  <th scope="col">Action</th>
                  <th scope="col">Actor</th>
                  <th scope="col">Target</th>
                  <th scope="col">Detail</th>
                </tr>
              </thead>
              <tbody>
                {data.entries.map((e) => (
                  <tr key={e.id}>
                    <td className="muted">{when(e.ts)}</td>
                    <td>
                      <code>{e.action}</code>
                    </td>
                    <td>
                      {e.actor_kind}
                      {e.actor_id ? (
                        <div className="muted" style={{ fontSize: 12 }}>
                          {e.actor_id.slice(0, 8)}
                        </div>
                      ) : null}
                    </td>
                    <td>
                      {e.target_kind}
                      {e.target_id ? (
                        <div className="muted" style={{ fontSize: 12 }}>
                          {e.target_id.slice(0, 8)}
                        </div>
                      ) : null}
                    </td>
                    <td>
                      <code className="muted" style={{ fontSize: 12 }}>
                        {JSON.stringify(e.detail).slice(0, 80)}
                      </code>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </TableFrame>

          {data.entries.length === 0 ? (
            <p className="muted">Nothing recorded in this window.</p>
          ) : null}

          {data.next_cursor ? (
            <button
              className="btn"
              style={{ marginTop: 12 }}
              data-testid="older"
              onClick={() => setBefore(data.next_cursor ?? undefined)}
            >
              Older
            </button>
          ) : (
            <p className="muted" style={{ marginTop: 12 }}>
              That is the whole log.
            </p>
          )}
        </>
      ) : null}
    </>
  );
}
