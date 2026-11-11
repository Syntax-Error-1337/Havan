import { useEffect, useMemo, useState } from 'react';
import { Link, useParams } from 'react-router-dom';
import {
  useProduct,
  useUpdateProduct,
  useValidateProduct,
  usePublishProduct,
  type RuleViolation,
} from '../../entities/product';
import { ApiError } from '../../shared/api/client';

/**
 * The editor is a JSON document with guard rails, not a form per capability.
 *
 * A thing model is a list of arbitrarily shaped mappings; a form would either
 * limit what a vendor can express or grow a widget per capability. The rules
 * are enforced server-side and rendered here, which is the part that matters.
 */
import { ManifestCard } from './ManifestCard';

export function ProductEditorPage() {
  const { id } = useParams<{ id: string }>();
  const { data: product, isPending, error } = useProduct(id);
  const update = useUpdateProduct(id ?? '');
  const validate = useValidateProduct(id ?? '');
  const publish = usePublishProduct(id ?? '');

  const [draft, setDraft] = useState('');
  const [parseError, setParseError] = useState<string | null>(null);
  const [violations, setViolations] = useState<RuleViolation[]>([]);
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    if (product) setDraft(JSON.stringify(product.thing_model, null, 2));
  }, [product]);

  const dirty = useMemo(
    () => Boolean(product) && draft !== JSON.stringify(product!.thing_model, null, 2),
    [draft, product],
  );

  if (isPending) return <p className="muted">Loading…</p>;
  if (error || !product) return <p className="error">{error?.message ?? 'Product not found.'}</p>;

  const save = async () => {
    setSaved(false);
    setParseError(null);
    try {
      const model = JSON.parse(draft);
      await update.mutateAsync({ thing_model: model });
      setSaved(true);
    } catch (e) {
      if (e instanceof SyntaxError) setParseError(e.message);
      else if (e instanceof ApiError) setParseError(e.message);
    }
  };

  const runValidation = async () => {
    setParseError(null);
    try {
      const result = await validate.mutateAsync();
      setViolations(result.violations);
    } catch (e) {
      if (e instanceof ApiError) setParseError(e.message);
    }
  };

  const onPublish = async () => {
    setParseError(null);
    try {
      await publish.mutateAsync();
      setViolations([]);
    } catch (e) {
      if (e instanceof ApiError) {
        setParseError(e.message);
        // The API returns every failing rule as a detail; show them as a list.
        const details = e.details ?? {};
        setViolations(
          Object.entries(details).map(([key, message]) => ({
            rule: Number(key.replace('rule_', '')) || 0,
            path: '',
            message,
          })),
        );
      }
    }
  };

  const readOnly = product.status === 'published';

  return (
    <>
      <div className="row spread" style={{ marginBottom: 20 }}>
        <div>
          <Link to="/" className="muted">
            ← Products
          </Link>
          <h2 style={{ margin: '4px 0 0' }}>{product.name}</h2>
          <p className="muted" style={{ margin: '4px 0 0' }}>
            <code>{product.product_key}</code> · <span className={`badge badge--${product.status}`}>{product.status}</span>
          </p>
        </div>

        <div className="row">
          <Link className="btn" to={`/products/${product.id}/firmware`}>
            Firmware
          </Link>
          {/* A published model is immutable, so there is nothing a check could
              find that a save could fix. */}
          <button
            className="btn"
            onClick={runValidation}
            disabled={readOnly || validate.isPending}
          >
            {validate.isPending ? 'Checking…' : 'Validate'}
          </button>
          <button className="btn" onClick={save} disabled={readOnly || !dirty || update.isPending}>
            {update.isPending ? 'Saving…' : 'Save draft'}
          </button>
          {/* No "Published" button: the status pill beside the key already says
              so, and a filled blue control that cannot be pressed reads as a
              broken button. */}
          {readOnly ? null : (
            <button className="btn btn--primary" onClick={onPublish} disabled={publish.isPending}>
              {publish.isPending ? 'Publishing…' : 'Publish'}
            </button>
          )}
        </div>
      </div>

      {saved && !dirty ? <p className="muted">Saved.</p> : null}
      {parseError ? <p className="error">{parseError}</p> : null}

      {violations.length > 0 ? (
        <div className="card" style={{ marginBottom: 20 }}>
          <strong>Not ready to publish</strong>
          <ul className="violations">
            {violations.map((v, i) => (
              <li key={i}>
                <code>rule {v.rule}</code> {v.message}
              </li>
            ))}
          </ul>
        </div>
      ) : null}

      {violations.length === 0 && validate.isSuccess ? (
        <div className="card" style={{ marginBottom: 20 }}>
          <span style={{ color: 'var(--success)' }}>✓ This model is valid.</span>
        </div>
      ) : null}

      <label className="field">
        <span>
          Thing model {readOnly ? <em className="muted">(published — immutable)</em> : null}
        </span>
        <textarea
          className="input textarea"
          aria-label="Thing model"
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          spellCheck={false}
          readOnly={readOnly}
        />
      </label>

      <ManifestCard productId={id ?? ''} />
    </>
  );
}
