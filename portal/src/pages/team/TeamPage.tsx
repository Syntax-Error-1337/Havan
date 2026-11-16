import { useState } from 'react';

import { useInvite, useRemoveMember, useTeam, type Role } from '../../entities/team';
import { TableFrame } from '../../shared/ui/TableFrame';

const ROLES: Array<Exclude<Role, 'owner'>> = ['admin', 'developer', 'viewer'];

/**
 * The team: who is in the vendor account, and who has been invited.
 *
 * The invitation token is shown exactly once, the way an API key is. There is no mail transport in
 * the platform yet, so this screen says so rather than pretending an email went out: the person
 * inviting has to send the link themselves.
 *
 * The role list is not filtered by the caller's own role here. The server enforces the rule (an
 * admin cannot create a peer, only an owner can), and duplicating it in the UI would mean two
 * places to keep in step — so the refusal is shown where it happens, in the form's error line.
 */
export function TeamPage() {
  const { data, isPending, error } = useTeam();
  const invite = useInvite();
  const remove = useRemoveMember();

  const [email, setEmail] = useState('');
  const [role, setRole] = useState<Exclude<Role, 'owner'>>('developer');
  const [token, setToken] = useState<string | null>(null);

  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    setToken(null);
    const result = await invite.mutateAsync({ email: email.trim(), role });
    setEmail('');
    setToken(result.token);
  };

  if (isPending) return <p className="muted">Loading team…</p>;
  if (error) return <p className="error">{error.message}</p>;

  return (
    <>
      <div className="row spread" style={{ marginBottom: 20 }}>
        <div>
          <h2 style={{ margin: 0 }}>Team</h2>
          <p className="muted" style={{ margin: '4px 0 0' }}>
            {data.members.length} member{data.members.length === 1 ? '' : 's'} · invitations expire
            after seven days
          </p>
        </div>
      </div>

      <form className="card" onSubmit={submit} style={{ marginBottom: 20 }}>
        <strong>Invite someone</strong>
        <div className="row" style={{ gap: 8, marginTop: 8, flexWrap: 'wrap' }}>
          <input
            className="input"
            type="email"
            required
            placeholder="name@company.com"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            aria-label="Email to invite"
          />
          <select
            className="input"
            value={role}
            onChange={(e) => setRole(e.target.value as Exclude<Role, 'owner'>)}
            aria-label="Role"
          >
            {ROLES.map((r) => (
              <option key={r} value={r}>
                {r}
              </option>
            ))}
          </select>
          <button className="btn btn--primary" type="submit" disabled={invite.isPending}>
            {invite.isPending ? 'Inviting…' : 'Create invitation'}
          </button>
        </div>
        {invite.error ? <p className="error">{invite.error.message}</p> : null}
        {token ? (
          <div style={{ marginTop: 12 }}>
            <p className="muted" style={{ margin: 0 }}>
              Send this link yourself — it is shown once and cannot be retrieved:
            </p>
            <code style={{ wordBreak: 'break-all' }} data-testid="invite-token">
              /v1/vendor/invites/accept?token={token}
            </code>
          </div>
        ) : null}
      </form>

      <TableFrame label="Team members">
        <table className="table">
          <thead>
            <tr>
              <th>Name</th>
              <th>Email</th>
              <th>Role</th>
              <th>Last login</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {data.members.map((m) => (
              <tr key={m.user_id}>
                <td>{m.display_name}</td>
                <td>{m.email}</td>
                <td>
                  <span className={`badge badge--${m.role}`}>{m.role}</span>
                </td>
                <td className="muted">
                  {m.last_login_at ? new Date(m.last_login_at).toLocaleString() : 'never'}
                </td>
                <td>
                  <button
                    className="btn"
                    onClick={() => remove.mutate(m.user_id)}
                    disabled={remove.isPending}
                  >
                    Remove
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </TableFrame>
      {remove.error ? <p className="error">{remove.error.message}</p> : null}

      <h3 style={{ marginTop: 24 }}>Pending invitations</h3>
      {data.invites.length === 0 ? (
        <p className="muted">Nobody is waiting to join.</p>
      ) : (
        <TableFrame label="Pending invitations">
          <table className="table">
            <thead>
              <tr>
                <th>Email</th>
                <th>Role</th>
                <th>Expires</th>
              </tr>
            </thead>
            <tbody>
              {data.invites.map((i) => (
                <tr key={i.id}>
                  <td>{i.email}</td>
                  <td>{i.role}</td>
                  <td className={i.expired ? 'error' : 'muted'}>
                    {i.expired ? 'expired' : new Date(i.expires_at).toLocaleDateString()}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </TableFrame>
      )}
    </>
  );
}
