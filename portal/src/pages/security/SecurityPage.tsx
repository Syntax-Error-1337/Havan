import { useState } from 'react';

import { useTeam } from '../../entities/team';
import {
  useBindSso,
  useConfirmTotp,
  useDisableTotp,
  useEnrolTotp,
  useSecurity,
  useSetSso,
  useSetTotpPolicy,
  useSso,
} from '../../entities/security';

/**
 * How the team signs in: the second factor and the identity provider.
 *
 * The one rule both halves follow, and the screen states it rather than hiding it behind a disabled
 * button: a policy may only be switched on when nobody is left unable to sign in.
 */
export function SecurityPage() {
  const security = useSecurity();
  const sso = useSso();

  const setPolicy = useSetTotpPolicy();
  const enrol = useEnrolTotp();
  const confirm = useConfirmTotp();
  const disable = useDisableTotp();
  const setSso = useSetSso();
  const team = useTeam();
  const bindSso = useBindSso();
  const [subjects, setSubjects] = useState<Record<string, string>>({});

  const [secret, setSecret] = useState<string | null>(null);
  const [uri, setUri] = useState<string | null>(null);
  const [code, setCode] = useState('');
  const [domain, setDomain] = useState('');

  if (security.isPending || sso.isPending) return <p className="muted">Loading security settings…</p>;
  if (security.error) return <p className="error">{security.error.message}</p>;
  if (sso.error) return <p className="error">{sso.error.message}</p>;

  const policy = security.data;
  const ssoConfig = sso.data;
  const everyoneEnrolled = policy.enrolled === policy.members;

  return (
    <>
      <h2 style={{ marginTop: 0 }}>Security</h2>
      <p className="muted" style={{ marginTop: -8 }}>
        Both switches below are refused while a member would be locked out by them.
      </p>

      <section className="card" style={{ marginBottom: 20 }}>
        <div className="row spread">
          <div>
            <strong>Two-factor authentication</strong>
            <p className="muted" style={{ margin: '4px 0 0' }}>
              {policy.enrolled} of {policy.members} members enrolled.
              {policy.totp_required ? ' Required for everyone.' : ' Optional today.'}
            </p>
          </div>
          <label className="row" style={{ gap: 8 }}>
            <input
              type="checkbox"
              checked={policy.totp_required}
              aria-label="Require two-factor for everyone"
              disabled={setPolicy.isPending}
              onChange={(e) => setPolicy.mutate(e.target.checked)}
            />
            <span>Require for everyone</span>
          </label>
        </div>

        {setPolicy.error ? <p className="error">{setPolicy.error.message}</p> : null}
        {!everyoneEnrolled ? (
          <p className="muted" style={{ margin: '12px 0 0' }}>
            Someone has not enrolled yet, so it cannot be required — turning it on would lock them
            out at their next sign-in.
          </p>
        ) : null}

        <hr style={{ border: 0, borderTop: '1px solid var(--border)', margin: '16px 0' }} />

        <strong>Your second factor</strong>
        {secret ? (
          <div style={{ marginTop: 8 }}>
            <p style={{ margin: '0 0 8px' }}>
              Add this secret to your authenticator app, then enter the code it shows. The secret is
              not shown again.
            </p>
            <code style={{ wordBreak: 'break-all' }} data-testid="totp-secret">
              {secret}
            </code>
            {uri ? (
              <p className="muted" style={{ margin: '8px 0 0', wordBreak: 'break-all' }}>
                {uri}
              </p>
            ) : null}
            <div className="row" style={{ marginTop: 12 }}>
              <input
                className="input"
                value={code}
                inputMode="numeric"
                aria-label="Six-digit code"
                placeholder="123456"
                onChange={(e) => setCode(e.target.value)}
              />
              <button
                className="btn btn--primary"
                disabled={code.trim().length !== 6 || confirm.isPending}
                onClick={async () => {
                  await confirm.mutateAsync(code.trim());
                  setSecret(null);
                  setUri(null);
                  setCode('');
                }}
              >
                Confirm code
              </button>
            </div>
            {confirm.error ? <p className="error">{confirm.error.message}</p> : null}
          </div>
        ) : (
          <div className="row" style={{ marginTop: 8 }}>
            <button
              className="btn"
              disabled={enrol.isPending}
              onClick={async () => {
                const started = await enrol.mutateAsync();
                setSecret(started.secret);
                setUri(started.otpauth_uri);
              }}
            >
              {policy.enrolled > 0 ? 'Replace my secret' : 'Set up authenticator'}
            </button>
            {policy.enrolled > 0 ? (
              <button className="btn" onClick={() => disable.mutate()}>
                Turn mine off
              </button>
            ) : null}
          </div>
        )}
        {enrol.error ? <p className="error">{enrol.error.message}</p> : null}
        {disable.error ? <p className="error">{disable.error.message}</p> : null}
      </section>

      <section className="card">
        <strong>Single sign-on</strong>
        <p className="muted" style={{ margin: '4px 0 12px' }}>
          {ssoConfig.enforced
            ? 'Enforced.'
            : 'Recorded, not enforced yet: we have no identity provider to validate an assertion against, so sign-in still uses a password.'}{' '}
          {ssoConfig.bound_members} of {ssoConfig.total_members} members have an identity bound.
          {ssoConfig.domain ? ` Recorded domain: ${ssoConfig.domain}.` : ' No domain recorded yet.'}
        </p>
        <div className="row" style={{ alignItems: 'flex-end' }}>
          <label className="field" style={{ flex: 1 }}>
            <span>Email domain</span>
            <input
              className="input"
              value={domain}
              placeholder={ssoConfig.domain ?? 'vendor.test'}
              aria-label="SSO email domain"
              onChange={(e) => setDomain(e.target.value)}
            />
          </label>
          <button
            className="btn"
            disabled={!domain.trim() || setSso.isPending}
            onClick={() => setSso.mutate({ required: ssoConfig.required, domain: domain.trim() })}
          >
            Save domain
          </button>
        </div>
        {setSso.error ? <p className="error">{setSso.error.message}</p> : null}

        <hr style={{ border: 0, borderTop: '1px solid var(--border)', margin: '16px 0' }} />

        {/*
          The step that was missing: enforcement is refused until every member is bound, and nothing
          in the product could bind one. A screen that offers the switch but not the way to satisfy
          it is a dead end, not a policy.
        */}
        <strong>Identities</strong>
        <p className="muted" style={{ margin: '4px 0 8px' }}>
          The subject is the stable identifier your identity provider issues for a person — not their
          email, which changes. {ssoConfig.bound_members} of {ssoConfig.total_members} bound.
        </p>
        <table className="table" data-testid="sso-members">
          <thead>
            <tr>
              <th>Member</th>
              <th>Role</th>
              <th>Identity</th>
              <th>Subject</th>
            </tr>
          </thead>
          <tbody>
            {(team.data?.members ?? []).map((m) => (
              <tr key={m.user_id} data-testid={`sso-member-${m.email}`}>
                <td>{m.email}</td>
                <td className="muted">{m.role}</td>
                <td>{m.sso_bound ? 'bound' : <span className="muted">not bound</span>}</td>
                <td>
                  <div className="row">
                    <input
                      className="input"
                      aria-label={`Subject for ${m.email}`}
                      placeholder={m.sso_bound ? 'replace…' : 'idp subject'}
                      value={subjects[m.user_id] ?? ''}
                      onChange={(e) =>
                        setSubjects((prev) => ({ ...prev, [m.user_id]: e.target.value }))
                      }
                    />
                    <button
                      className="btn"
                      disabled={!subjects[m.user_id]?.trim() || bindSso.isPending}
                      onClick={async () => {
                        await bindSso.mutateAsync({
                          userId: m.user_id,
                          subject: (subjects[m.user_id] ?? '').trim(),
                        });
                        setSubjects((prev) => ({ ...prev, [m.user_id]: '' }));
                      }}
                    >
                      Bind
                    </button>
                    {m.sso_bound ? (
                      <button
                        className="btn"
                        disabled={bindSso.isPending}
                        onClick={() => bindSso.mutate({ userId: m.user_id, subject: '' })}
                      >
                        Clear
                      </button>
                    ) : null}
                  </div>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        {bindSso.error ? <p className="error">{bindSso.error.message}</p> : null}
      </section>
    </>
  );
}
