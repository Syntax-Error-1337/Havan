import { Link, Navigate, Route, Routes, useLocation } from 'react-router-dom';
import { AdminShell } from './AdminShell';
import { useSession } from './session';
import { LoginPage } from '../pages/login/LoginPage';
import { ProductsPage } from '../pages/products/ProductsPage';
import { ProductEditorPage } from '../pages/product-editor/ProductEditorPage';
import { FirmwarePage } from '../pages/firmware/FirmwarePage';
import { CampaignsPage } from '../pages/campaigns/CampaignsPage';
import { ApiKeysPage } from '../pages/api-keys/ApiKeysPage';
import { SupportPage } from '../pages/support/SupportPage';
import { LicencesPage } from '../pages/licences/LicencesPage';
import { TeamPage } from '../pages/team/TeamPage';
import { FleetPage } from '../pages/fleet/FleetPage';
import { AnalyticsPage } from '../pages/analytics/AnalyticsPage';
import { AuditPage } from '../pages/audit/AuditPage';
import { InvoicesPage } from '../pages/invoices/InvoicesPage';
import { WebhooksPage } from '../pages/webhooks/WebhooksPage';
import { SecurityPage } from '../pages/security/SecurityPage';

/**
 * The gate lives here, not in each page: a new screen cannot forget it.
 */
export function AppShell() {
  const { token } = useSession();
  const location = useLocation();

  // The console has its own token and its own shell, and is routed before the vendor gate: an
  // operator must not be asked for a vendor password to reach a screen that manages vendors.
  if (location.pathname.startsWith('/admin')) {
    return <AdminShell />;
  }

  if (!token) {
    return (
      <Routes>
        <Route path="/login" element={<LoginPage />} />
        <Route path="*" element={<Navigate to="/login" replace />} />
      </Routes>
    );
  }

  return (
    <div className="shell">
      <header className="topbar">
        <span className="brand">Haven</span>
        <span className="muted">Vendor Portal</span>
        <nav className="row" style={{ marginLeft: 'auto', gap: 16 }}>
          <Link to="/" className="muted">
            Products
          </Link>
          <Link to="/fleet" className="muted">
            Fleet
          </Link>
          <Link to="/analytics" className="muted">
            Analytics
          </Link>
          <Link to="/audit" className="muted">
            Audit
          </Link>
          <Link to="/campaigns" className="muted">
            Rollouts
          </Link>
          <Link to="/licences" className="muted">
            Licences
          </Link>
          <Link to="/invoices" className="muted">
            Invoices
          </Link>
          <Link to="/team" className="muted">
            Team
          </Link>
          <Link to="/webhooks" className="muted">
            Webhooks
          </Link>
          <Link to="/security" className="muted">
            Security
          </Link>
          <Link to="/api-keys" className="muted">
            API keys
          </Link>
          <Link to="/support" className="muted">
            Support
          </Link>
        </nav>
      </header>
      <main className="content">
        <Routes>
          <Route path="/" element={<ProductsPage />} />
          <Route path="/products/:id" element={<ProductEditorPage />} />
          <Route path="/products/:id/firmware" element={<FirmwarePage />} />
          <Route path="/fleet" element={<FleetPage />} />
          <Route path="/analytics" element={<AnalyticsPage />} />
          <Route path="/audit" element={<AuditPage />} />
          <Route path="/campaigns" element={<CampaignsPage />} />
          {/* The nav calls it Rollouts, so the URL people type should work:
              otherwise a vendor who guesses the name lands on Products with no
              explanation. */}
          <Route path="/rollouts" element={<Navigate to="/campaigns" replace />} />
          <Route path="/api-keys" element={<ApiKeysPage />} />
          <Route path="/support" element={<SupportPage />} />
          <Route path="/licences" element={<LicencesPage />} />
          <Route path="/team" element={<TeamPage />} />
          <Route path="/invoices" element={<InvoicesPage />} />
          <Route path="/webhooks" element={<WebhooksPage />} />
          <Route path="/security" element={<SecurityPage />} />
          <Route path="/login" element={<Navigate to="/" replace />} />
          <Route path="*" element={<Navigate to="/" replace />} />
        </Routes>
      </main>
    </div>
  );
}
