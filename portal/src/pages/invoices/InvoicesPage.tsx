import { useState } from 'react';

import { formatMoney } from '../../entities/licence';
import { currentPeriod, useInvoice } from '../../entities/invoice';
import { TableFrame } from '../../shared/ui/TableFrame';

/**
 * What a vendor owes this month, and what we sent them for the months already issued.
 *
 * The distinction the screen refuses to blur: a **draft** still changes, because a device activated
 * late belongs to the month it activated in; an **issued** invoice does not, because it is a
 * document somebody may already have paid.
 */
export function InvoicesPage() {
  const [period, setPeriod] = useState(currentPeriod());
  const { data: invoice, isPending, error } = useInvoice(period);

  return (
    <>
      <div className="row spread" style={{ marginBottom: 20 }}>
        <div>
          <h2 style={{ margin: 0 }}>Invoices</h2>
          <p className="muted" style={{ margin: '4px 0 0' }}>
            One licence per device activated in the month. A draft is still open; an issued invoice
            is a document and will not change.
          </p>
        </div>
        <label className="field">
          <span>Month</span>
          <input
            className="input"
            type="month"
            value={period}
            aria-label="Invoice month"
            onChange={(e) => setPeriod(e.target.value || currentPeriod())}
          />
        </label>
      </div>

      {isPending ? (
        <p className="muted">Loading…</p>
      ) : error ? (
        <p className="error">{error.message}</p>
      ) : (
        <>
          <div className="card row spread" style={{ marginBottom: 20 }}>
            <div>
              <span className={`badge badge--${invoice.status === 'issued' ? 'ok' : 'warn'}`}>
                {invoice.status}
              </span>
              <p className="muted" style={{ margin: '8px 0 0' }}>
                {invoice.status === 'issued'
                  ? `Issued ${new Date(invoice.issued_at ?? '').toLocaleDateString()} — this document will not change.`
                  : 'Draft — activations later in the month still count.'}
              </p>
            </div>
            <div style={{ textAlign: 'right' }}>
              <div style={{ fontSize: 24, fontWeight: 600 }}>
                {formatMoney(invoice.amount_cents, invoice.currency)}
              </div>
              <p className="muted" style={{ margin: '4px 0 0' }}>
                {invoice.quantity} {invoice.quantity === 1 ? 'device' : 'devices'}
              </p>
            </div>
          </div>

          <TableFrame label={`Activations in ${invoice.period}`}>
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
                {invoice.lines.map((line) => (
                  <tr key={`${line.serial}-${line.activated_at}`}>
                    <td>
                      <code>{line.serial}</code>
                    </td>
                    <td>{line.product_key}</td>
                    <td className="muted">
                      {new Date(line.activated_at).toLocaleString(undefined, {
                        dateStyle: 'medium',
                        timeStyle: 'short',
                      })}
                    </td>
                    <td style={{ textAlign: 'right' }}>
                      {formatMoney(line.unit_price_cents, invoice.currency)}
                    </td>
                  </tr>
                ))}
                {invoice.lines.length === 0 ? (
                  <tr>
                    <td colSpan={4} className="muted">
                      No devices were activated in {invoice.period}.
                    </td>
                  </tr>
                ) : null}
              </tbody>
            </table>
          </TableFrame>
        </>
      )}
    </>
  );
}
