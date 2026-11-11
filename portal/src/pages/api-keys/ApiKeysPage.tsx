import { useState } from 'react';
import { keyState, useApiKeys, useCreateApiKey, useRevokeApiKey } from '../../entities/api-key';

/**
 * API keys for a vendor's own tooling.
 *
 * The page has one unusual job: showing a secret exactly once, clearly enough
 * that nobody closes the tab without copying it, and never again after that.
 */
export function ApiKeysPage() {
  const { data: keys, isPending, error } = useApiKeys();
  const create = useCreateApiKey();
  const revoke = useRevokeApiKey();

  const [name, setName] = useState('');
  const [minted, setMinted] = useState<string | null>(null);
  // 'failed' exists because a clipboard write can be refused — no permission, no
  // secure context, another window holding the clipboard. Claiming "Copied" when
  // we do not know would be the worst possible lie on this page.
  const [copied, setCopied] = useState<'idle' | 'ok' | 'failed'>('idle');

  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    setCopied('idle');
    const key = await create.mutateAsync({ name: name.trim() });
    setName('');
    setMinted(key.secret ?? null);
  };

  if (isPending) return <p className="muted">Loading keys…</p>;
  if (error) return <p className="error">{error.message}</p>;

  return (
    <>
      <div className="row spread" style={{ marginBottom: 20 }}>
        <div>
          <h2 style={{ margin: 0 }}>API keys</h2>
          <p className="muted" style={{ margin: '4px 0 0' }}>
            For your own factory or integration tooling. A key acts as your vendor
            account until you revoke it.
          </p>
        </div>
      </div>

      {minted ? (
        <div className="card" style={{ marginBottom: 20, borderColor: 'var(--accent)' }}>
          <strong>Copy this now</strong>
          {/* Not muted: this is the one sentence on the page that must be read. */}
          <p style={{ margin: '4px 0 12px' }}>
            This is the only time the secret is shown. Anyone with it can act as
            your vendor account.
          </p>
          <div className="row">
            <input className="input" readOnly value={minted} spellCheck={false} />
            <button
              className="btn"
              onClick={async (event) => {
                // currentTarget is null once the handler awaits; keep it now.
                const button = event.currentTarget;
                try {
                  await navigator.clipboard.writeText(minted);
                  setCopied('ok');
                } catch {
                  setCopied('failed');
                  // Fall back to something the person can act on: select the text
                  // so Ctrl+C works.
                  const field = button.parentElement?.querySelector('input');
                  field?.select();
                }
              }}
            >
              {copied === 'ok' ? 'Copied' : copied === 'failed' ? 'Select it — copy failed' : 'Copy'}
            </button>
            <button className="btn" onClick={() => setMinted(null)}>
              Done
            </button>
          </div>
        </div>
      ) : null}

      <form onSubmit={submit} className="card" style={{ marginBottom: 20 }}>
        <div className="row" style={{ alignItems: 'flex-end' }}>
          <label className="field" style={{ flex: 1 }}>
            <span>Name</span>
            <input
              className="input"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="factory line 2"
            />
          </label>
          <div className="field" style={{ flex: 0 }}>
            <span aria-hidden="true">&nbsp;</span>
            <button className="btn btn--primary" disabled={!name.trim() || create.isPending}>
              {create.isPending ? 'Minting…' : 'Create key'}
            </button>
          </div>
        </div>
        {create.error ? <p className="error">{create.error.message}</p> : null}
      </form>

      <div className="card" style={{ padding: 0 }}>
        {keys.length === 0 ? (
          <p className="muted" style={{ padding: 24 }}>
            No keys yet. Create one and copy the secret before you close the page.
          </p>
        ) : (
          <table className="table">
            <thead>
              <tr>
                <th>Name</th>
                <th>Prefix</th>
                <th>Created</th>
                <th>Last used</th>
                <th>State</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {keys.map((key) => {
                const state = keyState(key);
                return (
                  <tr key={key.id}>
                    <td>{key.name}</td>
                    <td>
                      <code className="muted">hv_{key.prefix}_…</code>
                    </td>
                    <td className="muted">
                      {new Date(key.created_at).toLocaleDateString()}
                    </td>
                    <td className="muted">
                      {key.last_used_at
                        ? new Date(key.last_used_at).toLocaleString()
                        : 'never'}
                    </td>
                    <td>
                      <span className={`badge badge--${state === 'revoked' ? 'halted' : 'running'}`}>
                        {state}
                      </span>
                    </td>
                    <td>
                      {state === 'revoked' ? (
                        <span className="muted">—</span>
                      ) : (
                        <button
                          className="btn"
                          disabled={revoke.isPending}
                          onClick={() => revoke.mutate(key.id)}
                        >
                          Revoke
                        </button>
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
