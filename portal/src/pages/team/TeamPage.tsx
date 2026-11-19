import { useState } from 'react';

import { useInvite, useRemoveMember, useRevokeInvite, useTeam, type Role } from '../../entities/team';
import { TableFrame } from '../../shared/ui/TableFrame';
import { storedToken } from '../../shared/api/client';
import { describeError } from '../../shared/api/errors';

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
  const revoke = useRevokeInvite();

  const [email, setEmail] = useState('');
  const [role, setRole] = useState<Exclude<Role, 'owner'>>('developer');
  const [token, setToken] = useState<string | null>(null);
  const selfId = currentUserId();
  const owners = data?.members.filter((m) => m.role === 'owner') ?? [];

  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    setToken(null);
    const result = await invite.mutateAsync({ email: email.trim(), role });
    setEmail('');
    setToken(result.token);
  };

  if (isPending) return <p className="muted">Loading team…</p>;
  if (error) return <p className="error">{describeError(error)}</p>;

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
              <th scope="col">Name</th>
              <th scope="col">Email</th>
              <th scope="col">Role</th>
              <th scope="col">Last login</th>
              <th scope="col" />
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
                  {/* Only the removals the server would accept are offered. It refuses two: yourself ("ask another
                      owner") and the last owner, whose departure would leave the account unowned. A button that can
                      only fail is a button that teaches nothing. */}
                  {m.user_id === selfId ? (
                    <span className="muted">You</span>
                  ) : m.role === 'owner' && owners.length === 1 ? (
                    <span className="muted" title="The account would have no owner">
                      Owner
                    </span>
                  ) : (
                    <button
                      className="btn"
                      onClick={() => {
                        // Removing somebody cannot be undone, and this is one press away from a list of colleagues.
                        if (
                          window.confirm(
                            `Remove ${m.display_name} (${m.email})? They lose access immediately.`,
                          )
                        ) {
                          remove.mutate(m.user_id);
                        }
                      }}
                      disabled={remove.isPending}
                    >
                      Remove
                    </button>
                  )}
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
                <th scope="col">Email</th>
                <th scope="col">Role</th>
                <th scope="col">Expires</th>
                <th scope="col" />
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
                  <td>
                    {/* An invitation is a way in for seven days. Withdrawing one sent to the wrong address is the only
                        way to take it back before it expires. */}
                    <button
                      className="btn"
                      disabled={revoke.isPending}
                      onClick={() => {
                        if (window.confirm(`Withdraw the invitation for ${i.email}?`)) {
                          revoke.mutate(i.id);
                        }
                      }}
                    >
                      Withdraw
                    </button>
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

/**
 * Who is signed in, read from the token's own subject.
 *
 * Unverified on purpose: the server decides what anybody may do, and this only decides which buttons are worth drawing.
 * A forged subject in a client's memory changes the picture on the screen and nothing else.
 */
function currentUserId(): string | null {
  const token = storedToken();
  const payload = token?.split('.')[1];
  if (!payload) return null;
  try {
    const decoded = JSON.parse(atob(payload.replace(/-/g, '+').replace(/_/g, '/')));
    return typeof decoded.sub === 'string' ? decoded.sub : null;
  } catch {
    return null;
  }
}
