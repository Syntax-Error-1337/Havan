/**
 * One figure with a label and a note.
 *
 * Extracted the second time it was written: the health page and the vendor page each had their own
 * copy, differing only in a font size — which is how two screens end up looking like two products.
 * Presentational and pure, so the browser specs are its test.
 */
export function Stat({
  label,
  value,
  note,
  tone,
}: {
  label: string;
  value: string;
  note: string;
  tone?: 'ok' | 'warn';
}) {
  return (
    <div className="card" style={{ minWidth: 150, flex: '1 1 150px' }}>
      <div className="muted" style={{ fontSize: 12 }}>
        {label}
      </div>
      <div
        style={{
          fontSize: 26,
          fontWeight: 600,
          color:
            tone === 'warn'
              ? 'var(--danger, #b42318)'
              : tone === 'ok'
                ? 'var(--ok, #1f7a4d)'
                : undefined,
        }}
      >
        {value}
      </div>
      <div className="muted" style={{ fontSize: 12 }}>
        {note}
      </div>
    </div>
  );
}
