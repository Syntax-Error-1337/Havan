import { describe, expect, it } from 'vitest';

import { titleFor } from './title';

/**
 * The tab is the one piece of a single-page app that forgets to change.
 *
 * Every route used to read "Haven — Vendor Portal", which is two problems: three open tabs looked identical, and a screen
 * reader announced the same context after every navigation. WCAG 2.4.2 asks the title to describe the page, and in a SPA
 * "the page" is whatever the router is showing.
 *
 * A pure function, so it is tested as one — and it is a `.test.ts`, which the vitest config did not include until the
 * accessibility work widened it.
 */
describe('titleFor', () => {
  it('names the screen the router is showing', () => {
    expect(titleFor('/')).toBe('Products — Haven');
    expect(titleFor('/fleet')).toBe('Fleet — Haven');
    expect(titleFor('/audit')).toBe('Audit log — Haven');
    expect(titleFor('/security')).toBe('Security — Haven');
  });

  it('keeps a detail screen under the section it belongs to', () => {
    // `/products/:id/firmware` is part of a product, not a second Products page.
    expect(titleFor('/products/01a1cdb7-9950-7468-be7f-ebed52f8a1b8')).toBe('Product — Haven');
    expect(titleFor('/products/01a1cdb7/firmware')).toBe('Product — Haven');
  });

  it('does not read `/admin` as the vendor audit log', () => {
    // The longest prefix wins: `/audit` and `/admin/audit` are different screens with different audiences.
    expect(titleFor('/admin/audit')).toBe('Audit — Haven');
    expect(titleFor('/admin/vendors')).toBe('Vendors — Haven');
    expect(titleFor('/admin')).toBe('Console — Haven');
  });

  it('falls back to the platform name rather than inventing one', () => {
    expect(titleFor('/nothing-here')).toBe('Haven');
    expect(titleFor('/rollouts')).toBe('Haven');
  });
});
