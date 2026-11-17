import { useState } from 'react';

import { usePrivacyDue, usePurge } from '../../entities/admin';
import { Stat } from '../../shared/ui/Stat';

/**
 * Data lifecycle, from the operator's chair.
 *
 * Erasure has had an endpoint since the DSAR work landed: `DELETE /me` tombstones a person, the
 * retention window is the longest of their vendors' grants, and the worker purges hourly. What was
 * missing is the ability to *look*: an operator asked "is the purge running?" could only read logs.
 * The count is the answer, and a non-zero count that stays non-zero is the alarm.
 */
export function AdminPrivacyPage() {
  const due = usePrivacyDue();
  const purge = usePurge();
  const [ranAt, setRanAt] = useState<string | null>(null);

  const count = due.data?.due ?? 0;
  const drained = ranAt !== null && count === 0;

  return (
    <>
      <h2 style={{ marginTop: 0 }}>Privacy</h2>
      <p className="muted" style={{ marginTop: -8 }}>
        A person who erased their account is kept as a tombstone until the longest retention window of
        the vendors whose devices they owned — a vendor may still be obliged to hold the record. The
        worker purges due tombstones hourly; this page exists so that claim can be checked.
      </p>

      <div className="row" style={{ gap: 12, flexWrap: 'wrap', marginBottom: 20 }}>
        <Stat
          label="Due now"
          value={String(count)}
          note="erased accounts past their window"
          tone={count > 0 ? 'warn' : 'ok'}
        />
        <Stat
          label="Purges run"
          value={ranAt ? '1' : '0'}
          note={ranAt ?? 'this session'}
        />
      </div>

      <section className="card">
        <strong>Run the purge</strong>
        <p className="muted" style={{ margin: '6px 0 12px' }}>
          The same call the hourly worker makes, batched at 500 so one click cannot hold a transaction
          open over a backlog. Safe to run twice: only tombstones whose window has passed are touched.
        </p>
        <button
          className="btn btn--primary"
          disabled={purge.isPending}
          data-testid="run-purge"
          onClick={() => {
            purge.mutate(undefined, {
              onSuccess: (report) => {
                setRanAt(`${report.purged.length} purged just now`);
              },
            });
          }}
        >
          {purge.isPending ? 'Purging…' : 'Purge due tombstones'}
        </button>
        {purge.error ? <p className="error">{purge.error.message}</p> : null}
        {ranAt ? (
          <p className="muted" style={{ margin: '10px 0 0' }} data-testid="purge-result">
            {ranAt}
            {drained ? ' — nothing left due.' : ''}
          </p>
        ) : null}
      </section>

      <section className="card" style={{ marginTop: 16 }}>
        <strong>What a purge keeps</strong>
        <p className="muted" style={{ margin: '6px 0 0' }}>
          Every deletion leaves a receipt in <code>data_requests</code> — when it happened, and how long
          the record was held — with no key back to the person. That row is how &quot;we deleted it on
          the 4th&quot; stays answerable after the logs rotate.
        </p>
      </section>
    </>
  );
}
