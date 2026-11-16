import { useState } from 'react';

import {
  invoiceCsvUrl,
  useGrantLicences,
  useIssueInvoice,
  useVendors,
} from '../../entities/admin';
import { currentPeriod } from '../../entities/invoice';

const money = (cents: number) => `$${(cents / 100).toFixed(2)}`;

/**
 * The two money actions the operator takes: authorising more activations, and issuing the month.
 *
 * Both are here together because they are the same conversation with a vendor — "here is what you
 * bought, here is the bill" — and splitting them across two screens invites issuing an invoice for a
 * quota nobody granted.
 */
export function AdminBillingPage() {
  const { data: vendors, isPending, error } = useVendors();
  const grant = useGrantLicences();
  const issue = useIssueInvoice();

  const [vendorId, setVendorId] = useState('');
  const [quantity, setQuantity] = useState('100');
  const [price, setPrice] = useState('249');
  const [period, setPeriod] = useState(currentPeriod());
  const [issued, setIssued] = useState<{
    period: string;
    status: string;
    quantity: number;
    amount_cents: number;
  } | null>(null);

  if (isPending) return <p className="muted">Loading vendors…</p>;
  if (error) return <p className="error">{error.message}</p>;

  const selected = vendorId || vendors[0]?.id || '';

  return (
    <>
      <h2 style={{ marginTop: 0 }}>Billing</h2>
      <p className="muted" style={{ marginTop: -8 }}>
        Grant activations, then issue the month. Issuing freezes the period: a device activated
        afterwards belongs to the next invoice.
      </p>

      <div className="card" style={{ marginBottom: 20 }}>
        <strong>Grant licences</strong>
        <div className="row" style={{ alignItems: 'flex-end', flexWrap: 'wrap', marginTop: 12 }}>
          <label className="field" style={{ flex: 2, minWidth: 220 }}>
            <span>Vendor</span>
            <select
              className="input"
              value={selected}
              aria-label="Vendor"
              onChange={(e) => setVendorId(e.target.value)}
            >
              {vendors.map((v) => (
                <option key={v.id} value={v.id}>
                  {v.name} ({v.used}/{v.granted})
                </option>
              ))}
            </select>
          </label>
          <label className="field" style={{ flex: 1, minWidth: 120 }}>
            <span>Quantity</span>
            <input
              className="input"
              inputMode="numeric"
              value={quantity}
              aria-label="Quantity"
              onChange={(e) => setQuantity(e.target.value)}
            />
          </label>
          <label className="field" style={{ flex: 1, minWidth: 120 }}>
            <span>Unit price (cents)</span>
            <input
              className="input"
              inputMode="numeric"
              value={price}
              aria-label="Unit price in cents"
              onChange={(e) => setPrice(e.target.value)}
            />
          </label>
          <button
            className="btn btn--primary"
            disabled={!selected || grant.isPending}
            onClick={() =>
              grant.mutate({
                vendor_id: selected,
                quantity: Number(quantity),
                unit_price_cents: Number(price),
              })
            }
          >
            {grant.isPending ? 'Granting…' : 'Grant'}
          </button>
        </div>
        {grant.error ? <p className="error">{grant.error.message}</p> : null}
      </div>

      <div className="card">
        <strong>Issue an invoice</strong>
        <div className="row" style={{ alignItems: 'flex-end', flexWrap: 'wrap', marginTop: 12 }}>
          <label className="field" style={{ flex: 1, minWidth: 180 }}>
            <span>Month</span>
            <input
              className="input"
              type="month"
              value={period}
              aria-label="Invoice period"
              onChange={(e) => setPeriod(e.target.value)}
            />
          </label>
          <button
            className="btn btn--primary"
            disabled={!selected || issue.isPending}
            onClick={async () => {
              const result = await issue.mutateAsync({ vendor_id: selected, period });
              setIssued(result);
            }}
          >
            {issue.isPending ? 'Issuing…' : 'Issue'}
          </button>
          <a
            className="btn"
            href={invoiceCsvUrl(selected, period)}
            download
            aria-label="Download invoice CSV"
          >
            CSV
          </a>
        </div>

        {issued ? (
          <p style={{ marginTop: 12 }} data-testid="issued-invoice">
            {issued.period}: <strong>{issued.status}</strong>, {issued.quantity} devices,{' '}
            {money(issued.amount_cents)}.
          </p>
        ) : null}
        {issue.error ? <p className="error">{issue.error.message}</p> : null}
      </div>
    </>
  );
}
