import { useState } from 'react';

import { getAdminToken, setAdminToken } from '../../shared/api/admin';

/**
 * The operator's door.
 *
 * On this machine the ops door accepts loopback without a token, so "Continue on this machine"
 * exists and is the common case in development. Anywhere else the token is required, and the screen
 * says what it is for rather than pretending to be a username and password.
 */
export function AdminLoginPage({ onSignedIn }: { onSignedIn: () => void }) {
  const [token, setToken] = useState(getAdminToken() ?? '');

  const submit = (event: React.FormEvent) => {
    event.preventDefault();
    setAdminToken(token.trim() || null);
    onSignedIn();
  };

  return (
    <div className="card" style={{ maxWidth: 520, margin: '10vh auto' }}>
      <h2 style={{ marginTop: 0 }}>Operator console</h2>
      <p className="muted">
        This is the platform side: vendors, certification and billing. Vendors sign in at the portal,
        not here.
      </p>

      <form onSubmit={submit}>
        <label className="field">
          <span>Ops token</span>
          <input
            className="input"
            type="password"
            value={token}
            placeholder="leave empty on this machine"
            aria-label="Ops token"
            onChange={(e) => setToken(e.target.value)}
          />
        </label>
        <p className="muted" style={{ marginTop: -4 }}>
          Required from anywhere but loopback; it is the same token the deploy sets as{' '}
          <code>OPS_TOKEN</code>.
        </p>
        <button className="btn btn--primary" type="submit">
          Open the console
        </button>
      </form>
    </div>
  );
}
