import { useEffect } from 'react';
import { useLocation } from 'react-router-dom';

/**
 * The tab, and the first thing a screen reader announces, should say where the person is.
 *
 * The portal is a single-page app with one static `<title>` in `index.html`, so every route read "Haven — Vendor Portal":
 * a vendor with Fleet, Licences and Audit open in three tabs could not tell them apart, and a screen reader announced the
 * same context on every navigation. WCAG 2.4.2 asks for a title that describes the page, and for a SPA "the page" is
 * whatever the router is showing.
 *
 * The mapping is by longest prefix, so `/products/:id/firmware` reads as part of the product rather than as the product
 * list, and an unknown path falls back to the shell's own name rather than inventing a heading.
 */
const NAMES: [string, string][] = [
  ['/admin/vendors', 'Vendors'],
  ['/admin/certification', 'Certification'],
  ['/admin/audit', 'Audit'],
  ['/admin', 'Console'],
  ['/products/', 'Product'],
  ['/fleet', 'Fleet'],
  ['/analytics', 'Analytics'],
  ['/audit', 'Audit log'],
  ['/campaigns', 'Rollouts'],
  ['/api-keys', 'API keys'],
  ['/support', 'Support'],
  ['/licences', 'Licences'],
  ['/team', 'Team'],
  ['/invoices', 'Invoices'],
  ['/webhooks', 'Webhooks'],
  ['/security', 'Security'],
  ['/login', 'Sign in'],
  ['/', 'Products'],
];

export function titleFor(pathname: string): string {
  const match = NAMES.find(([prefix]) =>
    prefix === '/' ? pathname === '/' : pathname.startsWith(prefix),
  );
  return match ? `${match[1]} — Haven` : 'Haven';
}

export function useDocumentTitle() {
  const { pathname } = useLocation();
  useEffect(() => {
    document.title = titleFor(pathname);
  }, [pathname]);
}
