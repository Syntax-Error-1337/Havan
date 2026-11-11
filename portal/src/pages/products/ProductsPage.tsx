import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useProducts, useCreateProduct, type ThingModel } from '../../entities/product';
import { ProductTable } from '../../widgets/product-table/ProductTable';
import { ApiError } from '../../shared/api/client';

const BLANK: ThingModel = {
  schema_version: 1,
  product_key: '',
  name: '',
  category: 'light',
  capabilities: [
    { name: 'switch', points: { power: { dp: '1', kind: 'bool' } } },
  ],
};

export function ProductsPage() {
  const navigate = useNavigate();
  const { data, isPending, error } = useProducts();
  const create = useCreateProduct();
  const [showNew, setShowNew] = useState(false);
  const [key, setKey] = useState('');
  const [name, setName] = useState('');

  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    const created = await create.mutateAsync({
      thing_model: { ...BLANK, product_key: key.trim(), name: name.trim() },
      publish: false,
    });
    setShowNew(false);
    navigate(`/products/${created.id}`);
  };

  if (isPending) return <p className="muted">Loading products…</p>;

  if (error) {
    return (
      <div className="card">
        <p className="error">{error.message}</p>
      </div>
    );
  }

  return (
    <>
      <div className="row spread" style={{ marginBottom: 20 }}>
        <div>
          <h2 style={{ margin: 0 }}>Products</h2>
          <p className="muted" style={{ margin: '4px 0 0' }}>
            {data.length} product{data.length === 1 ? '' : 's'} · a product must be published
            before a customer can pair it
          </p>
        </div>
        <button className="btn btn--primary" onClick={() => setShowNew((v) => !v)}>
          New product
        </button>
      </div>

      {showNew ? (
        <form onSubmit={submit} className="card" style={{ marginBottom: 20 }}>
          <div className="row" style={{ alignItems: 'flex-end' }}>
            <label className="field" style={{ flex: 1, marginBottom: 0 }}>
              <span>Product key</span>
              <input
                className="input"
                value={key}
                onChange={(e) => setKey(e.target.value)}
                placeholder="hv_acme_strip1"
                spellCheck={false}
              />
            </label>
            <label className="field" style={{ flex: 1, marginBottom: 0 }}>
              <span>Name</span>
              <input
                className="input"
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="Acme Light Strip"
              />
            </label>
            <button className="btn btn--primary" disabled={!key || !name || create.isPending}>
              {create.isPending ? 'Creating…' : 'Create draft'}
            </button>
          </div>
          {create.error ? (
            <p className="error" style={{ marginTop: 12 }}>
              {create.error instanceof ApiError ? create.error.message : 'Could not create.'}
            </p>
          ) : null}
        </form>
      ) : null}

      <div className="card" style={{ padding: 0 }}>
        {data.length === 0 ? (
          <p className="muted" style={{ padding: 24 }}>
            No products yet. Create one, then define what it can do.
          </p>
        ) : (
          <ProductTable products={data} />
        )}
      </div>
    </>
  );
}
