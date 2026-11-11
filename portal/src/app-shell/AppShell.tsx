import { Link, Navigate, Route, Routes } from 'react-router-dom';
import { useSession } from './session';
import { LoginPage } from '../pages/login/LoginPage';
import { ProductsPage } from '../pages/products/ProductsPage';
import { ProductEditorPage } from '../pages/product-editor/ProductEditorPage';
import { FirmwarePage } from '../pages/firmware/FirmwarePage';
import { CampaignsPage } from '../pages/campaigns/CampaignsPage';

/**
 * The gate lives here, not in each page: a new screen cannot forget it.
 */
export function AppShell() {
  const { token } = useSession();

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
          <Link to="/campaigns" className="muted">
            Rollouts
          </Link>
        </nav>
      </header>
      <main className="content">
        <Routes>
          <Route path="/" element={<ProductsPage />} />
          <Route path="/products/:id" element={<ProductEditorPage />} />
          <Route path="/products/:id/firmware" element={<FirmwarePage />} />
          <Route path="/campaigns" element={<CampaignsPage />} />
          <Route path="/login" element={<Navigate to="/" replace />} />
          <Route path="*" element={<Navigate to="/" replace />} />
        </Routes>
      </main>
    </div>
  );
}
