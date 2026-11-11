import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { request, ApiError } from '../../shared/api/client';
import { useSession } from '../../app-shell/session';

type Tokens = { access: string; refresh: string };

/**
 * Vendor login. Three fields because a portal account is scoped to a tenant:
 * the same email can exist at two vendors, and the vendor is part of the
 * credential, not a lookup detail.
 */
export function LoginPage() {
  const navigate = useNavigate();
  const { signIn } = useSession();

  const [vendorId, setVendorId] = useState(
    import.meta.env.VITE_DEMO_VENDOR_ID ?? '11111111-1111-7111-8111-111111111111',
  );
  const [email, setEmail] = useState('admin@acme.test');
  const [password, setPassword] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);

  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    setError(null);
    setPending(true);
    try {
      const tokens = await request<Tokens>('/vendor/auth/login', {
        method: 'POST',
        body: JSON.stringify({ vendor_id: vendorId.trim(), email: email.trim(), password }),
      });
      signIn(tokens.access);
      navigate('/', { replace: true });
    } catch (e) {
      setError(e instanceof ApiError ? e.message : 'Could not sign in.');
    } finally {
      setPending(false);
    }
  };

  return (
    <div style={{ maxWidth: 420, margin: '12vh auto', padding: 24 }}>
      <h1 style={{ marginBottom: 4 }}>Haven</h1>
      <p className="muted" style={{ marginTop: 0, marginBottom: 24 }}>
        Vendor portal
      </p>

      <form onSubmit={submit} className="card">
        <label className="field">
          <span>Vendor ID</span>
          <input
            className="input"
            value={vendorId}
            onChange={(e) => setVendorId(e.target.value)}
            autoComplete="off"
            spellCheck={false}
          />
        </label>

        <label className="field">
          <span>Email</span>
          <input
            className="input"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            autoComplete="username"
            type="email"
          />
        </label>

        <label className="field">
          <span>Password</span>
          <input
            className="input"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            autoComplete="current-password"
            type="password"
          />
        </label>

        {error ? <p className="error">{error}</p> : null}

        <button className="btn btn--primary" type="submit" disabled={pending} style={{ width: '100%' }}>
          {pending ? 'Signing in…' : 'Sign in'}
        </button>
      </form>
    </div>
  );
}
