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

  // The short name, not the id. This field used to ask for a 36-character UUID with a `pattern` that
  // refused anything else — a credential nobody can remember, check, or read out to a colleague. The
  // server accepts the slug or the id; the form asks for the slug because that is what a person knows.
  const [vendor, setVendor] = useState(import.meta.env.VITE_DEMO_VENDOR ?? 'acme');
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
        body: JSON.stringify({ vendor: vendor.trim(), email: email.trim(), password }),
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
          <span>Workspace</span>
          <input
            className="input"
            value={vendor}
            onChange={(e) => setVendor(e.target.value)}
            autoComplete="organization"
            spellCheck={false}
            required
            placeholder="acme"
            title="The short name of your vendor account, as in acme. A vendor ID works too."
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
            required
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
            required
          />
        </label>

        {error ? (
          <p className="error" role="alert">
            {error}
          </p>
        ) : null}

        <button className="btn btn--primary" type="submit" disabled={pending} style={{ width: '100%' }}>
          {pending ? 'Signing in…' : 'Sign in'}
        </button>
      </form>
    </div>
  );
}
