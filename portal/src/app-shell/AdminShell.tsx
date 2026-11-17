import { Link, Navigate, Route, Routes, useNavigate } from 'react-router-dom';
import { useQueryClient } from '@tanstack/react-query';

import { useOverview } from '../entities/admin';
import { AdminError, getAdminToken, setAdminToken } from '../shared/api/admin';
import { AdminBillingPage } from '../pages/admin/AdminBillingPage';
import { AdminHealthPage } from '../pages/admin/AdminHealthPage';
import { AdminPrivacyPage } from '../pages/admin/AdminPrivacyPage';
import { AdminCertificationPage } from '../pages/admin/AdminCertificationPage';
import { AdminLoginPage } from '../pages/admin/AdminLoginPage';
import { AdminVendorPage } from '../pages/admin/AdminVendorPage';
import { AdminVendorsPage } from '../pages/admin/AdminVendorsPage';

/**
 * The operator console, behind its own door.
 *
 * Separate from the vendor shell in every way that matters: its own token, its own header, its own
 * nav. An operator token and a vendor token deliberately never share storage — one XSS should not be
 * able to act as both.
 *
 * It does not ask for a token before it tries. On loopback the ops door accepts the request without
 * one, and a login screen in front of a door that is already open is theatre; the screen appears
 * only when the platform actually refuses.
 */
export function AdminShell() {
  const navigate = useNavigate();
  const qc = useQueryClient();
  const overview = useOverview();

  const refused =
    overview.error instanceof AdminError &&
    (overview.error.status === 401 || overview.error.status === 403);

  if (refused) {
    return (
      <AdminLoginPage
        onSignedIn={() => {
          // Re-read everything with the token just entered, rather than leaving the console in the
          // state it was refused in.
          void qc.invalidateQueries({ queryKey: ['admin'] });
          navigate('/admin');
        }}
      />
    );
  }

  return (
    <div className="shell">
      <header className="topbar">
        <span className="brand">Haven</span>
        <span className="muted">Operator console</span>
        <nav className="row" style={{ marginLeft: 'auto', gap: 16 }}>
          <Link to="/admin" className="muted">
            Vendors
          </Link>
          <Link to="/admin/health" className="muted">
            Health
          </Link>
          <Link to="/admin/certification" className="muted">
            Certification
          </Link>
          <Link to="/admin/privacy" className="muted">
            Privacy
          </Link>
          <Link to="/admin/billing" className="muted">
            Billing
          </Link>
          {getAdminToken() ? (
            <button
              className="btn"
              onClick={() => {
                setAdminToken(null);
                void qc.resetQueries({ queryKey: ['admin'] });
                navigate('/admin');
              }}
            >
              Sign out
            </button>
          ) : null}
        </nav>
      </header>

      <main className="content">
        {overview.data ? (
          <p className="muted" style={{ marginTop: 0 }} data-testid="admin-overview">
            {overview.data.vendors} vendors · {overview.data.suspended} suspended ·{' '}
            {overview.data.awaiting_certification} awaiting certification ·{' '}
            {overview.data.invoices_this_month}{' '}
            {overview.data.invoices_this_month === 1 ? 'invoice' : 'invoices'} this month
          </p>
        ) : null}

        {/* Absolute paths: this shell is mounted for every /admin/* URL, so a relative route would
            be matched against "/admin" itself and silently render nothing. */}
        <Routes>
          <Route path="/admin" element={<AdminVendorsPage />} />
          <Route path="/admin/vendors/:id" element={<AdminVendorPage />} />
          <Route path="/admin/health" element={<AdminHealthPage />} />
          <Route path="/admin/certification" element={<AdminCertificationPage />} />
          <Route path="/admin/billing" element={<AdminBillingPage />} />
          <Route path="/admin/privacy" element={<AdminPrivacyPage />} />
          <Route path="/admin/login" element={<Navigate to="/admin" replace />} />
          <Route path="*" element={<Navigate to="/admin" replace />} />
        </Routes>
      </main>
    </div>
  );
}
