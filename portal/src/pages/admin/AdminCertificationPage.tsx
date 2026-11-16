import { useCertificationQueue, useDecideCertification } from '../../entities/admin';
import { TableFrame } from '../../shared/ui/TableFrame';

/**
 * The queue we owe vendors an answer to.
 *
 * A vendor asked us to test a product; until somebody does, that product cannot be claimed by a
 * customer and the vendor has hardware they cannot sell. Oldest first, because the queue is a
 * promise about order rather than a list of interesting things.
 */
export function AdminCertificationPage() {
  const { data: queue, isPending, error } = useCertificationQueue();
  const decide = useDecideCertification();

  if (isPending) return <p className="muted">Loading the queue…</p>;
  if (error) return <p className="error">{error.message}</p>;

  return (
    <>
      <h2 style={{ marginTop: 0 }}>Certification queue</h2>
      <p className="muted" style={{ marginTop: -8 }}>
        Publishing is the vendor's promise; certification is ours. Nothing here is claimable until
        somebody signs it off.
      </p>

      <TableFrame label="Awaiting certification">
        <table className="table">
          <thead>
            <tr>
              <th>Product</th>
              <th>Vendor</th>
              <th>Asked</th>
              <th>Note</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {queue.map((item) => (
              <tr key={item.product_id} data-testid={`queue-${item.product_key}`}>
                <td>
                  <strong>{item.name}</strong>
                  <div className="muted">
                    <code>{item.product_key}</code> · {item.category}
                  </div>
                </td>
                <td>{item.vendor_name}</td>
                <td className="muted">
                  {item.submitted_at ? new Date(item.submitted_at).toLocaleDateString() : '—'}
                </td>
                <td className="muted">{item.note ?? '—'}</td>
                <td style={{ textAlign: 'right' }}>
                  <button
                    className="btn btn--primary"
                    disabled={decide.isPending}
                    onClick={() =>
                      decide.mutate({ productId: item.product_id, decision: 'certify' })
                    }
                  >
                    Certify
                  </button>
                </td>
              </tr>
            ))}
            {queue.length === 0 ? (
              <tr>
                <td colSpan={5} className="muted">
                  Nothing waiting. Every submitted product has an answer.
                </td>
              </tr>
            ) : null}
          </tbody>
        </table>
      </TableFrame>

      {decide.error ? <p className="error">{decide.error.message}</p> : null}

      <p className="muted" style={{ marginTop: 16 }}>
        A device already in someone's home is never affected by a revocation — revoking stops new
        claims, and the existing fleet keeps working. The consumer is not collateral.
      </p>
    </>
  );
}
