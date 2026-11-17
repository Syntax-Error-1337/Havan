import { useState } from 'react';

import { formatMoney } from '../../entities/licence';
import { currentPeriod, useInvoice, type InvoiceLine } from '../../entities/invoice';
import { TableFrame } from '../../shared/ui/TableFrame';

/**
 * What a vendor owes this month, and what we sent them for the months already issued.
 *
 * The distinction the screen refuses to blur: a **draft** still changes, because a device activated
 * late belongs to the month it activated in; an **issued** invoice does not, because it is a
 * document somebody may already have paid.
 */
/**
 * What the rows add up to, by price.
 *
 * An invoice for a mixed month used to render 169 rows and one total, so a vendor scrolling the
 * document saw a column of `0.00 USD` — the devices activated against a free grant — and had no way to
 * tell a free activation from a device that failed to bill. Grouping by price answers both: the money
 * reconciles, and every zero is explained as a zero.
 */
export function priceGroups(lines: InvoiceLine[]): { price: number; count: number }[] {
  const byPrice = new Map<number, number>();
  for (const line of lines) byPrice.set(line.unit_price_cents, (byPrice.get(line.unit_price_cents) ?? 0) + 1);
  return [...byPrice.entries()]
    .map(([price, count]) => ({ price, count }))
    .sort((a, b) => b.price - a.price);
}

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

          {/* The rows add up on screen, not only in the total. */}
          <p className="muted" data-testid="invoice-reconciliation">
            {priceGroups(invoice.lines)
              .map(({ price, count }) =>
                price === 0
                  ? `${count} ${count === 1 ? 'device' : 'devices'} at no charge`
                  : `${count} ${count === 1 ? 'device' : 'devices'} at ${formatMoney(price, invoice.currency)} = ${formatMoney(count * price, invoice.currency)}`,
              )
              .join(' · ')}
            {invoice.lines.length > 0 ? ` · total ${formatMoney(invoice.amount_cents, invoice.currency)}` : ''}
          </p>

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
                      {/* A zero is a fact; "0.00 USD" reads as a pricing bug. These devices were
                          activated against a grant that carries no per-device price. */}
                      {line.unit_price_cents === 0
                        ? 'No charge'
                        : formatMoney(line.unit_price_cents, invoice.currency)}
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
              {invoice.lines.length > 0 ? (
                <tfoot>
                  <tr>
                    <th colSpan={3} style={{ textAlign: 'right' }}>
                      Total · {invoice.quantity} {invoice.quantity === 1 ? 'device' : 'devices'}
                    </th>
                    <th style={{ textAlign: 'right' }}>
                      {formatMoney(invoice.amount_cents, invoice.currency)}
                    </th>
                  </tr>
                </tfoot>
              ) : null}
            </table>
          </TableFrame>
        </>
      )}
    </>
  );
}
