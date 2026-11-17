import { useState } from 'react';

import { download } from '../../shared/api/client';

/**
 * What a vendor sends us when something is wrong.
 *
 * `GET /vendor/support-bundle` has existed since the support work landed and no screen ever called
 * it, which made it a route only a curl could reach. The bundle is the difference between a ticket
 * that says "it does not work" and one that arrives with the product, the firmware, the fleet shape
 * and the counters already in it.
 */
export function SupportPage() {
  const [state, setState] = useState<'idle' | 'working' | 'done'>('idle');
  const [error, setError] = useState<string | null>(null);

  const save = async () => {
    setError(null);
    setState('working');
    try {
      await download('/vendor/support-bundle', 'haven-support-bundle.json');
      setState('done');
    } catch (e) {
      setState('idle');
      setError(e instanceof Error ? e.message : 'the bundle could not be produced');
    }
  };

  return (
    <>
      <h2 style={{ marginTop: 0 }}>Support</h2>
      <p className="muted" style={{ marginTop: -8 }}>
        A bundle is the fastest way to get a problem looked at: it carries the shape of your fleet,
        not its contents.
      </p>

      <section className="card">
        <strong>Support bundle</strong>
        <p className="muted" style={{ margin: '6px 0 12px' }}>
          Products, firmware versions, device states and counters. It is built from an allowlist, so a
          field that is not on the list cannot be included by accident — and it never contains a
          customer&apos;s name, email or address. A redacted row is not evidence.
        </p>
        <button
          className="btn btn--primary"
          disabled={state === 'working'}
          onClick={() => void save()}
          data-testid="download-bundle"
        >
          {state === 'working' ? 'Building…' : 'Download bundle'}
        </button>
        {state === 'done' ? (
          <p className="muted" style={{ margin: '10px 0 0' }} data-testid="bundle-done">
            Saved. Attach it to your support request.
          </p>
        ) : null}
        {error ? <p className="error">{error}</p> : null}
      </section>

      <section className="card" style={{ marginTop: 16 }}>
        <strong>Regenerating or firmware issues</strong>
        <p className="muted" style={{ margin: '6px 0 0' }}>
          The bundle includes each device&apos;s last reported firmware and its online state as the
          platform believes it — if a device is reported offline and you can see it powered, say so
          in the ticket: that difference is usually the bug.
        </p>
      </section>
    </>
  );
}
