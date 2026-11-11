import { useState } from 'react';

import { parseSerials, useSerials, useUploadSerials } from '../../entities/serials';

/**
 * The manufacturing manifest, as a place to paste serials.
 *
 * This is the only screen a vendor uses once and then comes back to every time
 * they build a batch, so it says how many are left rather than how many exist.
 */
export function ManifestCard({ productId }: { productId: string }) {
  const { data: summary } = useSerials(productId);
  const upload = useUploadSerials(productId);

  const [text, setText] = useState('');
  const [result, setResult] = useState<string | null>(null);

  const serials = parseSerials(text);

  const submit = (event: React.FormEvent) => {
    event.preventDefault();
    setResult(null);
    upload.mutate(serials, {
      onSuccess: (r) =>
        setResult(
          `${r.added} added` +
            (r.already_listed > 0 ? `, ${r.already_listed} were already listed` : ''),
        ),
      onError: (e) => setResult(e instanceof Error ? e.message : 'Upload failed'),
    });
    setText('');
  };

  return (
    <form className="card" style={{ marginTop: 20 }} onSubmit={submit}>
      <strong>Manufacturing manifest</strong>
      <p className="muted" style={{ margin: '4px 0 8px' }}>
        The serials you actually built. A product that requires a manifest will only
        pair a device whose serial is on this list.
      </p>

      {/* Its own line, not the tail of the paragraph: this number is the reason
          the vendor opened the screen. */}
      {summary ? (
        <p style={{ margin: '0 0 12px' }}>
          <strong>
            {summary.open} of {summary.total}
          </strong>{' '}
          <span className="muted">still unclaimed</span>
        </p>
      ) : null}

      <textarea
        className="input textarea"
        placeholder={'SN-0001\nSN-0002\nSN-0003'}
        value={text}
        onChange={(e) => setText(e.target.value)}
        rows={3}
        spellCheck={false}
      />

      <div className="row" style={{ marginTop: 12 }}>
        <button className="btn btn--primary" type="submit" disabled={serials.length === 0 || upload.isPending}>
          {upload.isPending ? 'Uploading…' : `Upload ${serials.length || ''}`.trim()}
        </button>
        {result ? <span className="muted">{result}</span> : null}
      </div>
    </form>
  );
}
