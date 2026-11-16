import { Link, useParams } from 'react-router-dom';

import {
  usePatchVendor,
  useVendor,
  useVendorInvoice,
  useVendorLicences,
} from '../../entities/admin';
import { currentPeriod } from '../../entities/invoice';
import { formatMoney } from '../../entities/licence';
import { Stat } from '../../shared/ui/Stat';
import { TableFrame } from '../../shared/ui/TableFrame';

/**
 * One vendor, everything an operator needs when that vendor calls.
 *
 * Composed entirely from reads that already exist — their balance and activations, the current
 * month's invoice, the row from the vendor list. The point is not new information; it is not having
 * to open a database client to answer "what do we owe each other" while somebody is on the phone.
 */
export function AdminVendorPage() {
  const { id = '' } = useParams();
  const vendor = useVendor(id);
  const licences = useVendorLicences(id);
  const period = currentPeriod();
  const invoice = useVendorInvoice(id, period);
  const patch = usePatchVendor();

  if (vendor.isPending) return <p className="muted">Loading vendor…</p>;
  if (vendor.error) return <p className="error">{vendor.error.message}</p>;

  const v = vendor.data;
  const s = licences.data?.summary;

  return (
    <>
      <p style={{ marginTop: 0 }}>
        <Link to="/admin" className="muted">
          ← Vendors
        </Link>
      </p>

      <div className="row spread" style={{ marginBottom: 20 }}>
        <div>
          <h2 style={{ margin: 0 }} data-testid="vendor-heading">
            {v.name}
          </h2>
          <p className="muted" style={{ margin: '4px 0 0' }}>
            <code>{v.slug}</code> · joined {new Date(v.created_at).toLocaleDateString()}
          </p>
        </div>
        <div className="row" style={{ gap: 8 }}>
          <span className={`badge badge--${v.status === 'active' ? 'ok' : 'danger'}`}>
            {v.status}
          </span>
          <button
            className="btn"
            disabled={patch.isPending}
            onClick={() =>
              patch.mutate({ id: v.id, status: v.status === 'active' ? 'suspended' : 'active' })
            }
          >
            {v.status === 'active' ? 'Suspend' : 'Restore'}
          </button>
        </div>
      </div>

      <div className="row" style={{ gap: 16, flexWrap: 'wrap', marginBottom: 20 }}>
        <Stat label="Granted" value={s ? String(s.granted) : '—'} note="authorised, unexpired" />
        <Stat label="Activated" value={s ? String(s.used) : '—'} note="devices live, all time" />
        <Stat
          label="Remaining"
          value={s ? String(s.remaining) : '—'}
          note="granted − activated"
          tone={s && s.remaining < 0 ? 'warn' : undefined}
        />
        <Stat label="This month" value={s ? String(s.activated_this_period) : '—'} note="activations" />
        <Stat
          label="Quota"
          value={v.quota_devices === null ? 'none' : String(v.quota_devices)}
          note={`${v.devices} devices live`}
        />
        <Stat label="Products" value={String(v.products)} note="in the catalog" />
      </div>

      <div className="card" style={{ marginBottom: 20 }} data-testid="vendor-invoice">
        <strong>{period} invoice</strong>
        {invoice.isPending ? (
          <p className="muted">Reading…</p>
        ) : invoice.error ? (
          <p className="error">{invoice.error.message}</p>
        ) : (
          <p className="muted" style={{ margin: '4px 0 0' }}>
            {invoice.data.status} · {invoice.data.quantity} devices ·{' '}
            {formatMoney(invoice.data.amount_cents, invoice.data.currency)}
            {invoice.data.status === 'issued'
              ? ' — issued, will not change'
              : ' — draft, still counts activations'}
            . Issue it from <Link to="/admin/billing">Billing</Link>.
          </p>
        )}
      </div>

      <h3 style={{ marginBottom: 8 }}>Recent activations</h3>
      <TableFrame label="Recent activations">
        <table className="table">
          <thead>
            <tr>
              <th>Serial</th>
              <th>Product</th>
              <th>Activated</th>
              <th style={{ textAlign: 'right' }}>Unit price</th>
            </tr>
          </thead>
          <tbody>
            {(licences.data?.activations ?? []).map((a) => (
              <tr key={`${a.serial}-${a.activated_at}`}>
                <td>
                  <code>{a.serial}</code>
                </td>
                <td>{a.product_name}</td>
                <td className="muted">
                  {new Date(a.activated_at).toLocaleString(undefined, {
                    dateStyle: 'medium',
                    timeStyle: 'short',
                  })}
                </td>
                <td style={{ textAlign: 'right' }}>
                  {s ? formatMoney(a.unit_price_cents, s.currency) : `${a.unit_price_cents}¢`}
                </td>
              </tr>
            ))}
            {(licences.data?.activations ?? []).length === 0 ? (
              <tr>
                <td colSpan={4} className="muted">
                  Nothing activated yet.
                </td>
              </tr>
            ) : null}
          </tbody>
        </table>
      </TableFrame>

      {patch.error ? <p className="error">{patch.error.message}</p> : null}
    </>
  );
}
