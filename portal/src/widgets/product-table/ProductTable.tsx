import { Link } from 'react-router-dom';
import type { Product } from '../../entities/product';
import { TableFrame } from '../../shared/ui/TableFrame';

/**
 * The list is the vendor's whole catalogue. Status is the only column that
 * changes, and it is the one that decides whether customers can pair.
 */
export function ProductTable({ products }: { products: Product[] }) {
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
            </tr>
          ))}
        </tbody>
      </table>
</TableFrame>
  );
}
