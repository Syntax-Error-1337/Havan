import { Link } from 'react-router-dom';
import type { Product } from '../../entities/product';
import { TableFrame } from '../../shared/ui/TableFrame';

/**
 * The list is the vendor's whole catalogue. Status is the only column that
 * changes, and it is the one that decides whether customers can pair.
 */
export function ProductTable({
  products,
  onDelete,
}: {
  products: Product[];
  /** Present only where deleting is offered; the refusal comes from the server. */
  onDelete?: (product: Product) => void;
}) {
  return (
    <TableFrame label="Products">
  <table className="table">
        <thead>
          <tr>
            <th>Product</th>
            <th>Key</th>
            <th>Category</th>
            <th>Capabilities</th>
            <th>Status</th>
            <th>Certification</th>
            {onDelete ? <th aria-label="Actions" /> : null}
          </tr>
        </thead>
        <tbody>
          {products.map((product) => (
            <tr key={product.id}>
              <td>
                <Link to={`/products/${product.id}`}>{product.name}</Link>
              </td>
              <td>
                <code className="muted">{product.product_key}</code>
              </td>
              <td>{product.category}</td>
              <td>{product.thing_model.capabilities.length}</td>
              <td>
                <span className={`badge badge--${product.status}`}>{product.status}</span>
              </td>
              <td>
                {/* Two gates, deliberately separate: publishing is the vendor's promise,
                    certification is ours. A vendor can only ask for the second. */}
                <span className={`badge badge--cert-${product.certification}`}>
                  {product.certification}
                </span>
              </td>
              {onDelete ? (
                <td>
                  {/* Open first, Delete quiet: the row's job is the product, not its removal. */}
                  <div className="row" style={{ gap: 4, justifyContent: 'flex-end' }}>
                    <Link className="btn btn--quiet" to={`/products/${product.id}`} aria-label={`Open ${product.name}`}>
                      Open
                    </Link>
                    <button
                      className="btn btn--danger-quiet"
                      aria-label={`Delete ${product.name}`}
                      onClick={() => onDelete(product)}
                    >
                      Delete
                    </button>
                  </div>
                </td>
              ) : null}
            </tr>
          ))}
        </tbody>
      </table>
</TableFrame>
  );
}
