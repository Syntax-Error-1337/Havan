import { useFleetStats } from '../../entities/fleet';
import { TableFrame } from '../../shared/ui/TableFrame';
import { describeError } from '../../shared/api/errors';

/**
 * Fleet health, as counts.
 *
 * Every number here is a `count(*)` over rows the vendor can list device by device —
 * no sampling, no estimates, and no chart that implies more precision than the data
 * has. The two questions this screen answers are "how much of my fleet is alive" and
 * "is the rollout landing".
 */
export function AnalyticsPage() {
  const { data, isPending, error, refetch } = useFleetStats();

  if (isPending) return <p className="muted">Counting…</p>;
  if (error) {
    return (
      <div className="card">
        <p className="error">{describeError(error)}</p>
        <button onClick={() => void refetch()}>Try again</button>
      </div>
    );
  }

  const total = data.devices.total;
  const onlinePct = total === 0 ? 0 : Math.round((data.devices.online / total) * 100);

  return (
    <div>
      <h1>Analytics</h1>

      <div className="row" style={{ gap: 16, flexWrap: 'wrap' }}>
        <Stat label="Devices" value={total} />
        <Stat label="Online" value={`${data.devices.online} (${onlinePct}%)`} />
        <Stat label="Bound" value={data.devices.bound} />
        <Stat label="Unbound" value={data.devices.unbound} />
        <Stat label="Claimed, 7 days" value={data.new_devices.last_7d} />
        <Stat label="Claimed, 30 days" value={data.new_devices.last_30d} />
      </div>

      <h2>By product</h2>
      {data.by_product.length === 0 ? (
        <p className="muted">No devices yet.</p>
      ) : (
        <TableFrame label="Devices by product">
  <table className="table">
            <thead>
              <tr>
                <th scope="col">Product</th>
                <th scope="col">Devices</th>
                <th scope="col">Online</th>
              </tr>
            </thead>
            <tbody>
              {data.by_product.map((row) => (
                <tr key={row.product_key}>
                  <td>
                    {row.name} <span className="muted mono">{row.product_key}</span>
                  </td>
                  <td>{row.total}</td>
                  <td>{row.online}</td>
                </tr>
              ))}
            </tbody>
          </table>
</TableFrame>
      )}

      <h2>Firmware</h2>
      {data.firmware.length === 0 ? (
        <p className="muted">No device has reported a version yet.</p>
      ) : (
        <TableFrame label="Firmware distribution">
  <table className="table">
            <thead>
              <tr>
                <th scope="col">Version</th>
                <th scope="col">Devices</th>
              </tr>
            </thead>
            <tbody>
              {data.firmware.map((row) => (
                <tr key={row.version ?? 'unknown'}>
                  <td className="mono">{row.version ?? 'never reported'}</td>
                  <td>{row.devices}</td>
                </tr>
              ))}
            </tbody>
          </table>
</TableFrame>
      )}
    </div>
  );
}

function Stat({ label, value }: { label: string; value: string | number }) {
  return (
    <div className="card" style={{ minWidth: 140 }}>
      <div className="muted">{label}</div>
      <div style={{ fontSize: 28, fontWeight: 600 }}>{value}</div>
    </div>
  );
}
