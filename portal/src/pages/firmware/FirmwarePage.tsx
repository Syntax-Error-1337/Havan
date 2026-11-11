import { useState } from 'react';
import { Link, useParams } from 'react-router-dom';
import { useProduct } from '../../entities/product';
import { useAddFirmware, useFirmware } from '../../entities/ota';
import { ApiError } from '../../shared/api/client';

/**
 * Firmware for one product.
 *
 * The image is uploaded elsewhere and registered here: the API never carries the
 * binary, so all this form collects is what the device needs to verify it —
 * version, size, digest and signature.
 */
export function FirmwarePage() {
  const { id } = useParams<{ id: string }>();
  const { data: product } = useProduct(id);
  const { data: images, isPending, error } = useFirmware(id);
  const add = useAddFirmware(id);

  const [open, setOpen] = useState(false);
  const [version, setVersion] = useState('');
  const [size, setSize] = useState('');
  const [sha256, setSha256] = useState('');
  const [sig, setSig] = useState('');
  const [objectKey, setObjectKey] = useState('');
  const [problem, setProblem] = useState<string | null>(null);

  const digestLooksRight = /^[0-9a-fA-F]{64}$/.test(sha256);
  const canSubmit =
    version.trim().length > 0 &&
    digestLooksRight &&
    Number(size) > 0 &&
    sig.trim().length > 0 &&
    objectKey.trim().length > 0;

  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    setProblem(null);
    try {
      await add.mutateAsync({
        version: version.trim(),
        size: Number(size),
        sha256,
        ed25519_sig: sig.trim(),
        object_key: objectKey.trim(),
      });
      setOpen(false);
      setVersion('');
      setSize('');
      setSha256('');
      setSig('');
      setObjectKey('');
    } catch (e) {
      setProblem(e instanceof ApiError ? e.message : 'Could not register that image.');
    }
  };

  return (
    <>
      <div className="row spread" style={{ marginBottom: 20 }}>
        <div>
          <Link to="/" className="muted">
            ← Products
          </Link>
          <h2 style={{ margin: '4px 0 0' }}>{product?.name ?? 'Product'} — firmware</h2>
          <p className="muted" style={{ margin: '4px 0 0' }}>
            A device only installs an image whose digest and signature match.
          </p>
        </div>
        <div className="row">
          <button className="btn btn--primary" onClick={() => setOpen((v) => !v)}>
            Register image
          </button>
          <Link className="btn" to="/campaigns">
            Rollouts
          </Link>
        </div>
      </div>

      {open ? (
        <form onSubmit={submit} className="card" style={{ marginBottom: 20 }}>
          <div className="row">
            <label className="field" style={{ flex: 1 }}>
              <span>Version</span>
              <input
                className="input"
                value={version}
                onChange={(e) => setVersion(e.target.value)}
                placeholder="1.4.0"
              />
            </label>
            <label className="field" style={{ flex: 1 }}>
              <span>Size (bytes)</span>
              <input
                className="input"
                type="number"
                value={size}
                onChange={(e) => setSize(e.target.value)}
                placeholder="812344"
              />
            </label>
          </div>

          <label className="field">
            <span>SHA-256</span>
            <input
              className="input"
              value={sha256}
              onChange={(e) => setSha256(e.target.value)}
              spellCheck={false}
              placeholder="64 hex characters"
            />
            {sha256.length > 0 && !digestLooksRight ? (
              // Caught here as well as server-side: the round trip would say the
              // same thing a second later, and a vendor is typing, not testing.
              <span className="error" style={{ fontSize: 12 }}>
                a sha256 is 64 hex characters ({sha256.length} typed)
              </span>
            ) : null}
          </label>

          <label className="field">
            <span>Ed25519 signature</span>
            <input
              className="input"
              value={sig}
              onChange={(e) => setSig(e.target.value)}
              spellCheck={false}
              placeholder="base64, over the digest"
            />
          </label>

          <label className="field">
            <span>Object key</span>
            <input
              className="input"
              value={objectKey}
              onChange={(e) => setObjectKey(e.target.value)}
              spellCheck={false}
              placeholder="fw/acme/bulb/1.4.0.bin"
            />
          </label>

          {problem ? <p className="error">{problem}</p> : null}

          <button className="btn btn--primary" disabled={!canSubmit || add.isPending}>
            {add.isPending ? 'Registering…' : 'Register'}
          </button>
        </form>
      ) : null}

      <div className="card" style={{ padding: 0 }}>
        {isPending ? (
          <p className="muted" style={{ padding: 24 }}>
            Loading…
          </p>
        ) : error ? (
          <p className="error" style={{ padding: 24 }}>
            {error.message}
          </p>
        ) : (images ?? []).length === 0 ? (
          <p className="muted" style={{ padding: 24 }}>
            No firmware registered. A rollout needs at least one image.
          </p>
        ) : (
          <table className="table">
            <thead>
              <tr>
                <th>Version</th>
                <th>Size</th>
                <th>Digest</th>
                <th>Registered</th>
              </tr>
            </thead>
            <tbody>
              {(images ?? []).map((image) => (
                <tr key={image.id}>
                  <td>
                    <code>{image.version}</code>
                  </td>
                  <td>{(image.size / 1024).toFixed(0)} KB</td>
                  <td>
                    <code className="muted">{image.sha256.slice(0, 12)}…</code>
                  </td>
                  <td className="muted">{new Date(image.created_at).toLocaleDateString()}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </>
  );
}
