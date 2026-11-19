import { formatMoney, useLicences } from '../../entities/licence';
import { TableFrame } from '../../shared/ui/TableFrame';

/**
 * What the vendor bought, what they have spent, and the devices behind it.
 *
 * The one number that must never be hidden is `remaining` when it is negative: that is what an
 * operator revoking a grant after devices were live looks like, and a screen that clamps it to
 * zero would show a vendor something comfortable and untrue.
 */
export function LicencesPage() {
  const { data, isPending, error } = useLicences();

  if (isPending) return <p className="muted">Loading licences…</p>;
  if (error) return <p className="error">{error.message}</p>;

  const { summary, activations } = data;
  const short = summary.remaining < 0;

  return (
    <>
      <div className="row spread" style={{ marginBottom: 20 }}>
        <div>
          <h2 style={{ margin: 0 }}>Device licences</h2>
          <p className="muted" style={{ margin: '4px 0 0' }}>
            One licence per device that goes live in the Haven app. Charged when a device is
            activated, not when a serial is manufactured.
          </p>
        </div>
      </div>

      <div className="row" style={{ gap: 12, marginBottom: 20, flexWrap: 'wrap' }}>
        <div className="card" style={{ minWidth: 140 }}>
          <p className="muted" style={{ margin: 0 }}>
            Granted
          </p>
          <strong style={{ fontSize: 24 }}>{summary.granted}</strong>
        </div>
        <div className="card" style={{ minWidth: 140 }}>
          <p className="muted" style={{ margin: 0 }}>
            Activated
          </p>
          <strong style={{ fontSize: 24 }}>{summary.used}</strong>
        </div>
        <div className="card" style={{ minWidth: 140 }}>
          <p className="muted" style={{ margin: 0 }}>
            Remaining
          </p>
          <strong style={{ fontSize: 24, color: short ? 'var(--danger, #b42318)' : undefined }}>
            {summary.remaining}
          </strong>
        </div>
        <div className="card" style={{ minWidth: 140 }}>
          <p className="muted" style={{ margin: 0 }}>
            This month
          </p>
          <strong style={{ fontSize: 24 }}>{summary.activated_this_period}</strong>
        </div>
      </div>

      {short ? (
        <p className="error" style={{ marginBottom: 16 }}>
          {summary.used} devices are activated against {summary.granted} licences: the balance is
          negative, so new claims are refused until more are granted.
        </p>
      ) : null}

      {/* Three numbers, reconciled.
          This line used to read "498.00 USD authorised at 2.49 USD per device", which is 200 x 2.49 — the
          whole grant — printed directly under a sentence saying the charge happens when a device is
          *activated*. A vendor reading a money page does the other multiplication first: 185 x 2.49 is
          460.65, and a page that shows 498 next to 185 activated looks like it overcharged them by 37.35.
          Both numbers are true, and the page now says which is which and what is left. */}
      <p className="muted">
        {summary.granted_value_cents > 0 ? (
          <>
            Granted value {formatMoney(summary.granted_value_cents, summary.currency)} (
            {summary.granted} × {formatMoney(summary.unit_price_cents, summary.currency)}) ·
            consumed by {summary.used} activations{' '}
            {formatMoney(summary.used * summary.unit_price_cents, summary.currency)} · unused{' '}
            {formatMoney(
              Math.max(0, summary.granted_value_cents - summary.used * summary.unit_price_cents),
              summary.currency,
            )}
            .
          </>
        ) : (
          'No priced grant yet, so nothing has been invoiced.'
        )}
      </p>

      <h3 style={{ marginTop: 24 }}>Recent activations</h3>
      {activations.length === 0 ? (
        <p className="muted">No device has been activated yet.</p>
      ) : (
        <TableFrame label="Device activations">
          <table className="table">
            <thead>
              <tr>
                <th scope="col">Serial</th>
                <th scope="col">Product</th>
                <th scope="col">Activated</th>
                <th scope="col">Unit price</th>
              </tr>
            </thead>
            <tbody>
              {activations.map((a) => (
                <tr key={`${a.serial}-${a.activated_at}`}>
                  <td>
                    <code>{a.serial}</code>
                  </td>
                  <td>
                    {a.product_name} <code className="muted">{a.product_key}</code>
                  </td>
                  <td>{new Date(a.activated_at).toLocaleString()}</td>
                  <td>{formatMoney(a.unit_price_cents, summary.currency)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </TableFrame>
      )}
    </>
  );
}
