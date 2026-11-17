import { Navigate, NavLink, Route, Routes, useLocation } from 'react-router-dom';
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
        <nav className="row nav-links" style={{ marginLeft: 'auto', gap: 16 }}>
          <NavLink
  to="/"
  className={({ isActive }) => (isActive ? 'nav-link nav-link--active' : 'nav-link')}
>
  Products
</NavLink>
<NavLink
  to="/fleet"
  className={({ isActive }) => (isActive ? 'nav-link nav-link--active' : 'nav-link')}
>
  Fleet
</NavLink>
<NavLink
  to="/analytics"
  className={({ isActive }) => (isActive ? 'nav-link nav-link--active' : 'nav-link')}
>
  Analytics
</NavLink>
<NavLink
  to="/audit"
  className={({ isActive }) => (isActive ? 'nav-link nav-link--active' : 'nav-link')}
>
  Audit
</NavLink>
<NavLink
  to="/campaigns"
  className={({ isActive }) => (isActive ? 'nav-link nav-link--active' : 'nav-link')}
>
  Rollouts
</NavLink>
<NavLink
  to="/licences"
  className={({ isActive }) => (isActive ? 'nav-link nav-link--active' : 'nav-link')}
>
  Licences
</NavLink>
<NavLink
  to="/invoices"
  className={({ isActive }) => (isActive ? 'nav-link nav-link--active' : 'nav-link')}
>
  Invoices
</NavLink>
<NavLink
  to="/team"
  className={({ isActive }) => (isActive ? 'nav-link nav-link--active' : 'nav-link')}
>
  Team
</NavLink>
<NavLink
  to="/webhooks"
  className={({ isActive }) => (isActive ? 'nav-link nav-link--active' : 'nav-link')}
>
  Webhooks
</NavLink>
<NavLink
  to="/security"
  className={({ isActive }) => (isActive ? 'nav-link nav-link--active' : 'nav-link')}
>
  Security
</NavLink>
<NavLink
  to="/api-keys"
  className={({ isActive }) => (isActive ? 'nav-link nav-link--active' : 'nav-link')}
>
  API keys
</NavLink>
<NavLink
  to="/support"
  className={({ isActive }) => (isActive ? 'nav-link nav-link--active' : 'nav-link')}
>
  Support
</NavLink>
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
