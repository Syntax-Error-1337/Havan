import { useState } from 'react';
import { Link } from 'react-router-dom';

import { useCreateVendor, usePatchVendor, useVendors, type VendorRow } from '../../entities/admin';
import { TableFrame } from '../../shared/ui/TableFrame';

/**
 * The operator's book of business: who sells here, what they have live, and the one switch that
 * stops them selling more without touching a single device in a customer's home.
 */
export function AdminVendorsPage() {
  const { data: vendors, isPending, error } = useVendors();
  const create = useCreateVendor();
  const patch = usePatchVendor();

  const [showNew, setShowNew] = useState(false);
  const [slug, setSlug] = useState('');
  const [name, setName] = useState('');

  // Searching the vendor book is the operator's most common act: a support call starts with a name, a slug
  // from a log line, or "who is suspended". Fifty-five rows and no way to ask a question about them is a
  // list, not a console.
  const [q, setQ] = useState('');
  const [only, setOnly] = useState<'all' | 'active' | 'suspended'>('all');

  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    await create.mutateAsync({ slug: slug.trim(), name: name.trim() });
    setSlug('');
    setName('');
    setShowNew(false);
  };

  if (isPending) return <p className="muted">Loading vendors…</p>;
  if (error) return <p className="error">{error.message}</p>;

  const suspended = vendors.filter((v) => v.status === 'suspended').length;

  const needle = q.trim().toLowerCase();
  const rows = vendors.filter(
    (v) =>
      (only === 'all' || v.status === only) &&
      (needle === '' ||
        v.name.toLowerCase().includes(needle) ||
        v.slug.toLowerCase().includes(needle)),
  );

  return (
    <>
      <div className="row spread" style={{ marginBottom: 20 }}>
        <div>
          <h2 style={{ margin: 0 }}>Vendors</h2>
          <p className="muted" style={{ margin: '4px 0 0' }} data-testid="vendor-counts">
            {vendors.length} vendors · {suspended} suspended
          </p>
        </div>
        <button className="btn btn--primary" onClick={() => setShowNew((v) => !v)}>
          Onboard vendor
        </button>
      </div>

      {showNew ? (
        <form onSubmit={submit} className="card" style={{ marginBottom: 20 }}>
          <div className="row" style={{ alignItems: 'flex-end', flexWrap: 'wrap' }}>
            <label className="field" style={{ flex: 1, minWidth: 200 }}>
              <span>Slug</span>
              <input
                className="input"
                value={slug}
                placeholder="acme-home"
                aria-label="Vendor slug"
                onChange={(e) => setSlug(e.target.value)}
              />
            </label>
            <label className="field" style={{ flex: 1, minWidth: 200 }}>
              <span>Name</span>
              <input
                className="input"
                value={name}
                placeholder="Acme Home"
                aria-label="Vendor name"
                onChange={(e) => setName(e.target.value)}
              />
            </label>
            <button className="btn btn--primary" disabled={!slug.trim() || !name.trim() || create.isPending}>
              {create.isPending ? 'Creating…' : 'Create vendor'}
            </button>
          </div>
          {create.error ? <p className="error">{create.error.message}</p> : null}
        </form>
      ) : null}

      <div className="row spread" style={{ marginBottom: 12, gap: 12 }}>
        <input
          className="input"
          style={{ maxWidth: 320 }}
          placeholder="Search a vendor name or slug"
          aria-label="Search vendors"
          value={q}
          onChange={(e) => setQ(e.target.value)}
        />
        <div className="row" style={{ gap: 6 }}>
          {(['all', 'active', 'suspended'] as const).map((value) => (
            <button
              key={value}
              className={`btn ${only === value ? 'btn--primary' : 'btn--quiet'}`}
              aria-pressed={only === value}
              onClick={() => setOnly(value)}
            >
              {value === 'all' ? 'All' : value === 'active' ? 'Active' : 'Suspended'}
            </button>
          ))}
        </div>
      </div>

      <TableFrame label="Vendors">
        <table className="table">
          <thead>
            <tr>
              <th>Vendor</th>
              <th>Status</th>
              <th style={{ textAlign: 'right' }}>Products</th>
              <th style={{ textAlign: 'right' }}>Devices</th>
              {/* Two different ceilings, and the old headers named neither: "Licences 0 / 0" is activated
                  of granted, and "Quota" is a device cap that refuses the *next* activation. */}
              <th style={{ textAlign: 'right' }} title="Devices activated / licences granted">
                Activated
              </th>
              <th style={{ textAlign: 'right' }} title="Device cap; the next activation is refused at the cap">
                Device cap
              </th>
              <th>
                <span className="muted">Actions</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {rows.map((v) => (
              <tr key={v.id} data-testid={`vendor-${v.slug}`}>
                <td>
                  <Link to={`/admin/vendors/${v.id}`}>
                    <strong>{v.name}</strong>
                  </Link>
                  <div className="muted">
                    <code>{v.slug}</code>
                  </div>
                </td>
                <td>
                  <span className={`badge badge--${v.status === 'active' ? 'ok' : 'danger'}`}>
                    {v.status}
                  </span>
                </td>
                <td style={{ textAlign: 'right' }}>{v.products}</td>
                <td style={{ textAlign: 'right' }}>{v.devices}</td>
                <td style={{ textAlign: 'right' }}>
                  {v.used.toLocaleString()} / {v.granted.toLocaleString()}
                </td>
                <td style={{ textAlign: 'right' }}>
                  {v.quota_devices === null || v.quota_devices === undefined
                    ? 'none'
                    : v.quota_devices.toLocaleString()}
                </td>
                <td style={{ textAlign: 'right' }}>
                  <button
                    className="btn"
                    disabled={patch.isPending}
                    onClick={() =>
                      patch.mutate({
                        id: v.id,
                        status: v.status === 'active' ? 'suspended' : 'active',
                      })
                    }
                  >
                    {v.status === 'active' ? 'Suspend' : 'Restore'}
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        {rows.length === 0 ? (
          <p className="muted" style={{ padding: 24 }} data-testid="no-vendors-match">
            No vendor matches that. {q ? 'Clear the search' : 'Pick another status'} to see them all.
          </p>
        ) : null}
      </TableFrame>

      <p className="muted" style={{ marginTop: 16 }}>
        Suspending stops <strong>new activations</strong> and makes that vendor's portal read-only.
        Devices already in people's homes keep working — they are still our customers. A{' '}
        <strong>quota</strong> is a second ceiling: when the live device count reaches it, the next
        activation is refused even if licences remain.
      </p>

      {patch.error ? <p className="error">{patch.error.message}</p> : null}
      <SuspensionNote vendor={vendors.find((v) => v.status === 'suspended')} />
    </>
  );
}

/** One line of proof that the screen and the promise agree. */
function SuspensionNote({ vendor }: { vendor: VendorRow | undefined }) {
  if (!vendor) return null;
  return (
    <p className="muted" data-testid="suspension-note">
      {vendor.name} is suspended: {vendor.devices} devices still online, no new activations.
    </p>
  );
}
