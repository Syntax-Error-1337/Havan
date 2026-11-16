import { useState } from 'react';

import {
  WEBHOOK_EVENTS,
  useSubscribeWebhook,
  useUnsubscribeWebhook,
  useWebhooks,
} from '../../entities/webhook';
import { TableFrame } from '../../shared/ui/TableFrame';

/**
 * Where a vendor's ERP waits for us to tell it something happened.
 *
 * The signing secret is shown once and the screen says what to do with it. There is no "resend"
 * because there is nothing to resend: the platform stores a hash, not the secret, exactly as it
 * does for invitations.
 */
export function WebhooksPage() {
  const { data: hooks, isPending, error } = useWebhooks();
  const subscribe = useSubscribeWebhook();
  const remove = useUnsubscribeWebhook();

  const [url, setUrl] = useState('');
  const [events, setEvents] = useState<string[]>([...WEBHOOK_EVENTS]);
  const [secret, setSecret] = useState<string | null>(null);

  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    const created = await subscribe.mutateAsync({ url: url.trim(), events });
    setUrl('');
    setSecret(created.secret ?? null);
  };

  const toggle = (name: string) =>
    setEvents((current) =>
      current.includes(name) ? current.filter((e) => e !== name) : [...current, name],
    );

  if (isPending) return <p className="muted">Loading subscriptions…</p>;
  if (error) return <p className="error">{error.message}</p>;

  return (
    <>
      <div className="row spread" style={{ marginBottom: 20 }}>
        <div>
          <h2 style={{ margin: 0 }}>Webhooks</h2>
          <p className="muted" style={{ margin: '4px 0 0' }}>
            Pushed to your endpoint, signed with HMAC-SHA256, retried with backoff. Nothing we write
            ever waits for your server.
          </p>
        </div>
      </div>

      {secret ? (
        <div className="card" style={{ marginBottom: 20, borderColor: 'var(--accent)' }}>
          <strong>Signing secret — copy it now</strong>
          <p style={{ margin: '4px 0 12px' }}>
            This is the only time it is shown. Verify every request's signature against it.
          </p>
          <div className="row" style={{ flexWrap: 'wrap' }}>
            <input
              className="input"
              style={{ minWidth: 0, flex: 1 }}
              readOnly
              value={secret}
              spellCheck={false}
            />
            <button className="btn" onClick={() => setSecret(null)}>
              Done
            </button>
          </div>
        </div>
      ) : null}

      <form onSubmit={submit} className="card" style={{ marginBottom: 20 }}>
        <label className="field">
          <span>Endpoint URL</span>
          <input
            className="input"
            value={url}
            required
            placeholder="https://erp.example.com/haven"
            aria-label="Endpoint URL"
            onChange={(e) => setUrl(e.target.value)}
          />
        </label>
        {/* Wrapping is the whole fix for a narrow window: three event names in one row push the
            page sideways instead of stacking, and a horizontal scrollbar on a form is a bug. */}
        <div className="row" style={{ gap: 16, margin: '12px 0', flexWrap: 'wrap' }}>
          {WEBHOOK_EVENTS.map((name) => (
            <label key={name} className="row" style={{ gap: 6 }}>
              <input
                type="checkbox"
                checked={events.includes(name)}
                aria-label={name}
                onChange={() => toggle(name)}
              />
              <code>{name}</code>
            </label>
          ))}
        </div>
        <button className="btn btn--primary" type="submit" disabled={!url.trim() || !events.length}>
          Subscribe
        </button>
        {subscribe.error ? <p className="error">{subscribe.error.message}</p> : null}
      </form>

      <TableFrame label="Subscriptions">
        <table className="table">
          <thead>
            <tr>
              <th>Endpoint</th>
              <th>Events</th>
              <th>Created</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {hooks.map((hook) => (
              <tr key={hook.id}>
                <td>
                  <code style={{ wordBreak: 'break-all' }}>{hook.url}</code>
                </td>
                <td className="muted">{hook.events.join(', ')}</td>
                <td className="muted">{new Date(hook.created_at).toLocaleDateString()}</td>
                <td style={{ textAlign: 'right' }}>
                  <button className="btn" onClick={() => remove.mutate(hook.id)}>
                    Remove
                  </button>
                </td>
              </tr>
            ))}
            {hooks.length === 0 ? (
              <tr>
                <td colSpan={4} className="muted">
                  No endpoints yet. Add one above and we will tell it when a device activates.
                </td>
              </tr>
            ) : null}
          </tbody>
        </table>
      </TableFrame>
    </>
  );
}
