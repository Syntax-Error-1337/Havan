import { useEffect, useState } from 'react';

import { useSaveTheme, useTheme, useThemeHistory, type PlatformTheme } from '../../entities/admin';

/**
 * The brand, from the operator's chair.
 *
 * White-label is the product's whole commercial story: a partner licences the platform and ships it
 * under their own name. That only works if the name and the colours are data — so this page writes
 * the one row every client reads, and the app picks it up on its next start.
 *
 * Three deliberate limits, all visible on the page rather than discovered later:
 * - **Two colours and an appearance.** Not a stylesheet. A white-label customer is buying a product
 *   that still behaves like a phone; letting them recolour surfaces and text is how a home screen
 *   becomes unreadable at 7am.
 * - **The text colour on the accent is computed, not chosen.** The server picks black or white by
 *   luminance, and the preview shows the same answer the app will get.
 * - **Nothing changes until Save.** A colour that applies while being dragged is a colour two
 *   operators see differently for as long as it takes one of them to decide.
 */
export function AdminThemePage() {
  const current = useTheme();
  const history = useThemeHistory();
  const save = useSaveTheme(localStorage.getItem('haven.admin.actor'));

  const [draft, setDraft] = useState<PlatformTheme | null>(null);

  // Only adopt the saved theme once, and never over an edit in progress.
  useEffect(() => {
    if (current.data && !draft) setDraft(current.data);
  }, [current.data, draft]);

  if (current.isPending || !draft) {
    return <p className="muted">Loading the platform theme…</p>;
  }
  if (current.isError) {
    return <p className="error">{current.error.message}</p>;
  }

  const dirty =
    draft.product_name !== current.data?.product_name ||
    draft.logo_text !== current.data?.logo_text ||
    draft.accent !== current.data?.accent ||
    draft.accent_secondary !== current.data?.accent_secondary ||
    draft.mode !== current.data?.mode ||
    (draft.support_email ?? '') !== (current.data?.support_email ?? '') ||
    (draft.status_url ?? '') !== (current.data?.status_url ?? '');

  const set = <K extends keyof PlatformTheme>(key: K, value: PlatformTheme[K]) =>
    setDraft({ ...draft, [key]: value });

  return (
    <>
      <h2 style={{ marginTop: 0 }}>Appearance</h2>
      <p className="muted" style={{ marginTop: -8 }}>
        What the customer app wears: its name, its initials, its two colours and whether it is light or
        dark. Every phone reads this on start — before sign-in, so the app is branded from the first
        frame — and keeps the last answer if it starts offline.
      </p>

      <div className="row" style={{ gap: 24, alignItems: 'flex-start', flexWrap: 'wrap' }}>
        <section className="card" style={{ flex: '1 1 380px', minWidth: 320 }}>
          <strong>Brand</strong>

          <label className="field">
            <span>Product name</span>
            <input
              data-testid="theme-name"
              value={draft.product_name}
              maxLength={32}
              onChange={(e) => set('product_name', e.target.value)}
            />
          </label>

          <label className="field">
            <span>Logo initials</span>
            <input
              data-testid="theme-logo"
              value={draft.logo_text}
              maxLength={3}
              onChange={(e) => set('logo_text', e.target.value)}
            />
          </label>

          <label className="field">
            <span>Accent</span>
            <div className="row" style={{ gap: 8 }}>
              <input
                type="color"
                aria-label="Accent colour picker"
                data-testid="theme-accent-picker"
                value={draft.accent}
                style={{ width: 52, height: 38, padding: 2 }}
                onChange={(e) => set('accent', e.target.value.toUpperCase())}
              />
              <input
                data-testid="theme-accent"
                value={draft.accent}
                onChange={(e) => set('accent', e.target.value.toUpperCase())}
                placeholder="#0A84FF"
                style={{ flex: 1 }}
              />
            </div>
          </label>

          <label className="field">
            <span>Second colour</span>
            <div className="row" style={{ gap: 8 }}>
              <input
                type="color"
                aria-label="Second colour picker"
                data-testid="theme-accent2-picker"
                value={draft.accent_secondary}
                style={{ width: 52, height: 38, padding: 2 }}
                onChange={(e) => set('accent_secondary', e.target.value.toUpperCase())}
              />
              <input
                data-testid="theme-accent2"
                value={draft.accent_secondary}
                onChange={(e) => set('accent_secondary', e.target.value.toUpperCase())}
                placeholder="#8B5CF6"
                style={{ flex: 1 }}
              />
            </div>
          </label>

          <div className="field">
            <span>Appearance</span>
            <div className="row" style={{ gap: 8 }}>
              {(['dark', 'light'] as const).map((mode) => (
                <button
                  key={mode}
                  type="button"
                  data-testid={`theme-mode-${mode}`}
                  className={draft.mode === mode ? 'btn btn--primary' : 'btn'}
                  aria-pressed={draft.mode === mode}
                  onClick={() => set('mode', mode)}
                >
                  {mode === 'dark' ? 'Dark' : 'Light'}
                </button>
              ))}
            </div>
          </div>

          {/* Not part of the brand, but the same row: one white-label deployment has one support address, and a
              customer whose device will not pair needs it inside the app rather than on a website they must find. */}
          <strong style={{ display: 'block', marginTop: 20 }}>Help</strong>
          <label className="field">
            <span>Support email</span>
            <input
              data-testid="theme-support"
              value={draft.support_email ?? ''}
              maxLength={120}
              placeholder="help@example.com"
              onChange={(e) => set('support_email', e.target.value)}
            />
          </label>
          <label className="field">
            <span>Status page</span>
            <input
              data-testid="theme-status"
              value={draft.status_url ?? ''}
              maxLength={200}
              placeholder="https://status.example.com"
              onChange={(e) => set('status_url', e.target.value)}
            />
            <small className="muted">
              Shown in the app's About screen. Must start with https:// — the app opens it in the browser.
            </small>
          </label>

          <div className="row" style={{ gap: 8, marginTop: 16 }}>
            <button
              className="btn btn--primary"
              data-testid="theme-save"
              disabled={!dirty || save.isPending}
              onClick={() =>
                save.mutate({
                  product_name: draft.product_name,
                  logo_text: draft.logo_text,
                  accent: draft.accent,
                  accent_secondary: draft.accent_secondary,
                  mode: draft.mode,
                  // Empty means "not set", which the server stores as NULL rather than an empty row.
                  support_email: (draft.support_email ?? '').trim(),
                  status_url: (draft.status_url ?? '').trim(),
                })
              }
            >
              {save.isPending ? 'Saving…' : 'Save theme'}
            </button>
            <button
              className="btn"
              type="button"
              disabled={!dirty}
              data-testid="theme-reset"
              onClick={() => current.data && setDraft(current.data)}
            >
              Discard
            </button>
            {dirty ? (
              <span className="muted" data-testid="theme-dirty">
                unsaved changes
              </span>
            ) : null}
          </div>

          {save.error ? (
            <p className="error" data-testid="theme-error">
              {save.error.message}
            </p>
          ) : null}
          {save.isSuccess && !dirty ? (
            <p className="muted" data-testid="theme-saved">
              Saved — phones pick this up on their next start.
            </p>
          ) : null}
        </section>

        <PhonePreview theme={draft} />

        <section className="card" style={{ flex: '1 1 280px', minWidth: 260 }}>
          <strong>Recent changes</strong>
          {history.data?.changes.length ? (
            <ul className="muted" style={{ paddingLeft: 18, margin: '10px 0 0' }} data-testid="theme-history">
              {history.data.changes.map((change, i) => (
                <li key={`${change.changed_at}-${i}`}>
                  {change.changed_by} changed{' '}
                  {change.fields.map((field) => FIELD_LABELS[field] ?? field).join(', ') || 'nothing'} ·{' '}
                  {new Date(change.changed_at).toLocaleString()}
                </li>
              ))}
            </ul>
          ) : (
            <p className="muted" style={{ margin: '10px 0 0' }}>
              No changes recorded yet.
            </p>
          )}
          <p className="muted" style={{ marginTop: 14 }}>
            Last saved{' '}
            {current.data?.updated_at
              ? `${new Date(current.data.updated_at).toLocaleString()} by ${current.data.updated_by ?? 'ops'}`
              : 'never — this is the built-in theme'}
            .
          </p>
        </section>
      </div>
    </>
  );
}

/** Wire names are for the API; a person reads labels. */
const FIELD_LABELS: Record<string, string> = {
  product_name: 'the name',
  logo_text: 'the initials',
  accent: 'the accent',
  accent_secondary: 'the second colour',
  mode: 'the appearance',
};

/** A phone-shaped swatch: the same accent, the same computed text colour the app will receive. */
function PhonePreview({ theme }: { theme: PlatformTheme }) {
  const dark = theme.mode === 'dark';
  const onAccent = contrastOn(theme.accent);

  return (
    <section
      className="card"
      style={{ flex: '0 0 268px', display: 'flex', flexDirection: 'column', gap: 10 }}
      data-testid="theme-preview"
    >
      <strong>Preview</strong>
      <div
        style={{
          borderRadius: 26,
          padding: 18,
          minHeight: 360,
          background: dark
            ? 'radial-gradient(120% 90% at 10% 0%, rgba(14,116,144,0.55), transparent 60%), radial-gradient(120% 90% at 100% 20%, rgba(109,40,217,0.5), transparent 55%), #080B16'
            : 'radial-gradient(120% 90% at 10% 0%, rgba(125,211,252,0.6), transparent 60%), radial-gradient(120% 90% at 100% 20%, rgba(167,139,250,0.45), transparent 55%), #EEF2FB',
          color: dark ? '#FFFFFF' : '#0B1020',
        }}
      >
        <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
          <div
            style={{
              width: 38,
              height: 38,
              borderRadius: 19,
              display: 'grid',
              placeItems: 'center',
              background: `${theme.accent}33`,
              border: `1px solid ${theme.accent}66`,
              fontSize: 13,
              fontWeight: 700,
            }}
          >
            {theme.logo_text}
          </div>
          <div>
            <div style={{ fontSize: 10, letterSpacing: 1.2, opacity: 0.6 }}>DASHBOARD</div>
            <div style={{ fontSize: 18, fontWeight: 700 }}>{theme.product_name || 'Product'}</div>
          </div>
        </div>

        <div style={{ display: 'flex', gap: 8, marginTop: 16 }}>
          {['Lights', 'Inside'].map((label, i) => (
            <div
              key={label}
              style={{
                flex: 1,
                borderRadius: 16,
                padding: 10,
                background: dark ? 'rgba(255,255,255,0.06)' : 'rgba(255,255,255,0.8)',
                border: `1px solid ${i === 0 ? theme.accent : theme.accent_secondary}55`,
              }}
            >
              <div style={{ fontSize: 10, opacity: 0.65 }}>{label}</div>
              <div style={{ fontSize: 15, fontWeight: 700, color: i === 0 ? theme.accent : theme.accent_secondary }}>
                {i === 0 ? '3 on' : '21.5°'}
              </div>
            </div>
          ))}
        </div>

        <div
          style={{
            marginTop: 12,
            borderRadius: 18,
            padding: 14,
            background: dark ? 'rgba(255,255,255,0.06)' : 'rgba(255,255,255,0.82)',
            border: `1px solid ${theme.accent}44`,
          }}
        >
          <div style={{ fontSize: 11, opacity: 0.7 }}>Room sensor</div>
          <div style={{ fontSize: 30, fontWeight: 700, color: theme.accent }}>21.5°C</div>
          <div
            style={{
              height: 34,
              marginTop: 8,
              borderRadius: 8,
              background: `linear-gradient(180deg, ${theme.accent}55, transparent)`,
            }}
          />
        </div>

        <button
          type="button"
          tabIndex={-1}
          style={{
            marginTop: 14,
            width: '100%',
            border: 0,
            borderRadius: 14,
            padding: '11px 0',
            fontWeight: 700,
            background: theme.accent,
            // The server's rule, applied here too: this is the colour the app will be told to use.
            color: onAccent,
          }}
        >
          Add a device
        </button>
      </div>
    </section>
  );
}

/**
 * The same luminance rule the server applies.
 *
 * Duplicated here on purpose: the preview has to show what the app will actually get, and a preview
 * that always assumed white text would be wrong exactly when the operator picks a pale accent.
 */
export function contrastOn(hex: string): string {
  const clean = hex.replace('#', '');
  const full = clean.length === 3 ? clean.split('').map((c) => c + c).join('') : clean;
  const channel = (i: number) => parseInt(full.slice(i, i + 2), 16) / 255;
  const lin = (c: number) => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
  const luminance = 0.2126 * lin(channel(0)) + 0.7152 * lin(channel(2)) + 0.0722 * lin(channel(4));
  return luminance > 0.179 ? '#000000' : '#FFFFFF';
}
