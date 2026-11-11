import { useState } from 'react';
import { Link } from 'react-router-dom';
import { useProducts } from '../../entities/product';
import {
  headline,
  percentDone,
  useCampaigns,
  useCreateCampaign,
  useFirmware,
  useHaltCampaign,
  useStartCampaign,
} from '../../entities/ota';
import { ApiError } from '../../shared/api/client';

/**
 * Rollouts. The table answers one question — "is my fleet up to date?" — and
 * answers it per campaign rather than in aggregate, because a rollout that
 * failed on nine devices and succeeded on nine hundred is not "50% fine".
 */
export function CampaignsPage() {
  const { data: rows, isPending, error } = useCampaigns();
  const { data: products } = useProducts();
  const start = useStartCampaign();
  const halt = useHaltCampaign();
  const create = useCreateCampaign();

  const [showNew, setShowNew] = useState(false);
  const [productId, setProductId] = useState('');
  const [firmwareId, setFirmwareId] = useState('');
  const [pct, setPct] = useState('5');
  const [notice, setNotice] = useState<string | null>(null);

  const { data: firmware } = useFirmware(productId || undefined);

  if (isPending) return <p className="muted">Loading rollouts…</p>;
  if (error) return <p className="error">{error.message}</p>;

  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    setNotice(null);
    try {
      await create.mutateAsync({
        product_id: productId,
        firmware_id: firmwareId,
        rollout_pct: Number(pct),
      });
      setShowNew(false);
      setFirmwareId('');
      setNotice('Draft created. Starting it queues the devices in the wave.');
    } catch (e) {
      setNotice(e instanceof ApiError ? e.message : 'Could not create the campaign.');
    }
  };

  return (
    <>
      <div className="row spread" style={{ marginBottom: 20 }}>
        <div>
          <h2 style={{ margin: 0 }}>Rollouts</h2>
          <p className="muted" style={{ margin: '4px 0 0' }}>
            A campaign updates a percentage of a product's fleet. Start with a
            small wave.
          </p>
        </div>
        <button className="btn btn--primary" onClick={() => setShowNew((v) => !v)}>
          New rollout
        </button>
      </div>

      {notice ? <p className="muted">{notice}</p> : null}

      {showNew ? (
        <form onSubmit={submit} className="card" style={{ marginBottom: 20 }}>
          <div className="row" style={{ alignItems: 'flex-end' }}>
            <label className="field" style={{ flex: 2 }}>
              <span>Product</span>
              <select
                className="input"
                value={productId}
                onChange={(e) => {
                  setProductId(e.target.value);
                  setFirmwareId('');
                }}
              >
                <option value="">Choose a product…</option>
                {(products ?? []).map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name}
                  </option>
                ))}
              </select>
            </label>

            <label className="field" style={{ flex: 2 }}>
              <span>Firmware</span>
              <select
                className="input"
                value={firmwareId}
                onChange={(e) => setFirmwareId(e.target.value)}
                disabled={!productId}
              >
                <option value="">
                  {(firmware ?? []).length === 0 ? 'None registered' : 'Choose…'}
                </option>
                {(firmware ?? []).map((f) => (
                  <option key={f.id} value={f.id}>
                    {f.version}
                  </option>
                ))}
              </select>
            </label>

            <label className="field" style={{ flex: 1 }}>
              <span>Rollout %</span>
              <input
                className="input"
                type="number"
                min={0}
                max={100}
                value={pct}
                onChange={(e) => setPct(e.target.value)}
              />
            </label>

            <div className="field" style={{ flex: 0 }}>
              <span aria-hidden="true">&nbsp;</span>
              <button
                className="btn btn--primary"
                disabled={!productId || !firmwareId || create.isPending}
              >
                {create.isPending ? 'Creating…' : 'Create draft'}
              </button>
            </div>
          </div>
        </form>
      ) : null}

      <div className="card" style={{ padding: 0 }}>
        {rows.length === 0 ? (
          <p className="muted" style={{ padding: 24 }}>
            No rollouts yet. Register firmware on a product, then start a campaign.
          </p>
        ) : (
          <table className="table">
            <thead>
              <tr>
                <th>Product</th>
                <th>Rollout</th>
                <th>State</th>
                <th>Progress</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((row) => {
                const product = (products ?? []).find((p) => p.id === row.campaign.product_id);
                const done = percentDone(row.progress);
                return (
                  <tr key={row.campaign.id}>
                    <td>{product?.name ?? row.campaign.product_id.slice(0, 8)}</td>
                    <td>{row.campaign.rollout_pct}%</td>
                    <td>
                      <span className={`badge badge--${row.campaign.status}`}>
                        {row.campaign.status}
                      </span>
                    </td>
                    <td>
                      <div
                        className={`progress ${
                          row.progress.failed > 0
                            ? 'progress--failed'
                            : row.complete
                              ? 'progress--clean'
                              : ''
                        }`}
                        title={`${done}% of ${row.progress.total}`}
                      >
                        <span style={{ width: `${done}%` }} />
                      </div>
                      <span className="muted" style={{ fontSize: 12 }}>
                        {headline(row)}
                      </span>
                    </td>
                    <td>
                      {row.campaign.status === 'draft' ? (
                        <button
                          className="btn btn--primary"
                          disabled={start.isPending}
                          onClick={() => start.mutate(row.campaign.id)}
                        >
                          Start
                        </button>
                      ) : row.campaign.status === 'running' ? (
                        // A rollout going badly needs a stop, and it needs it at
                        // 2am without a terminal.
                        <button
                          className="btn"
                          disabled={halt.isPending && halt.variables === row.campaign.id}
                          onClick={() => halt.mutate(row.campaign.id)}
                        >
                          {halt.isPending && halt.variables === row.campaign.id ? 'Halting…' : 'Halt'}
                        </button>
                      ) : (
                        <Link className="muted" to={`/products/${row.campaign.product_id}/firmware`}>
                          Firmware
                        </Link>
                      )}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </div>
    </>
  );
}
