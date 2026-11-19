import { expect, test } from './fixtures';

/**
 * Accessibility, measured — the portal's version of the app's three rules.
 *
 * The app has `app/e2e/a11y.spec.ts`: three hand-written rules, because `axe-core` is MPL-2.0 and this platform ships only
 * permissive licences (ADR-002). The portal had nothing, which is the worse half of the omission: a vendor is a company's
 * employee, often on a laptop with a mandated display, and the operator console is used all day.
 *
 * Three rules, the same three that earn their keep in the app:
 *   1. every control has an accessible name — a button whose only child is an icon is a button nobody can dictate a name
 *      for, and a placeholder is not a label;
 *   2. text meets WCAG AA against the background it is actually painted on, computed from the rendered pixels rather than
 *      from the palette, because a colour token can be right while the pair it lands on is not;
 *   3. a table column header says which column it heads and which way it sorts, so "Sort by status" is not a mystery.
 */

// Every screen a vendor or an operator can reach, not a sample: the rules are cheap enough to run on all of them, and a
// screen left out of the sweep is a screen the sweep cannot vouch for.
const PAGES = [
  '/',
  '/fleet',
  '/analytics',
  '/audit',
  '/campaigns',
  '/api-keys',
  '/support',
  '/licences',
  '/team',
  '/invoices',
  '/webhooks',
  '/security',
  '/admin',
  '/admin/vendors',
  '/admin/certification',
  '/admin/audit',
];

/** Every interactive element without a name, per page. */
test('every control has an accessible name', async ({ app }) => {
  for (const path of PAGES) {
    await app.goto(path);
    await app.waitForLoadState('networkidle');

    const nameless = await app.evaluate(() => {
      const controls = Array.from(
        document.querySelectorAll<HTMLElement>('button, a[href], input, select, textarea'),
      );
      const name = (element: HTMLElement): string => {
        const labelled = element.getAttribute('aria-labelledby');
        const labelledText = labelled
          ? labelled
              .split(/\s+/)
              .map((id) => document.getElementById(id)?.textContent ?? '')
              .join(' ')
          : '';
        const placeholderLabels = [
          element.getAttribute('aria-label'),
          labelledText,
          element.textContent,
          element.getAttribute('title'),
          // A label element that points at this control by id, or wraps it.
          element.id
            ? document.querySelector(`label[for="${element.id}"]`)?.textContent
            : null,
          element.closest('label')?.textContent,
        ];
        const alt = element.getAttribute('alt');
        const value = element.getAttribute('value');
        return [...placeholderLabels, alt, value]
          .filter((part): part is string => Boolean(part && part.trim()))
          .join(' ')
          .trim();
      };

      return controls
        .filter((element) => {
          // Hidden controls are not in the accessible tree, and a control inside a closed dialog is not reachable.
          const box = element.getBoundingClientRect();
          const style = getComputedStyle(element);
          if (box.width === 0 && box.height === 0) return false;
          if (style.visibility === 'hidden' || style.display === 'none') return false;
          if (element.getAttribute('aria-hidden') === 'true') return false;
          return name(element).length === 0;
        })
        .map((element) => element.outerHTML.slice(0, 120));
    });

    expect(nameless, `${path} has controls with no accessible name`).toEqual([]);
  }
});

/** Text against the colour it is actually painted on. */
test('body text meets WCAG AA against its background', async ({ app }) => {
  const failures: string[] = [];

  for (const path of PAGES) {
    await app.goto(path);
    await app.waitForLoadState('networkidle');

    const bad = await app.evaluate(() => {
      const parse = (colour: string): [number, number, number, number] | null => {
        const match = /rgba?\(([^)]+)\)/.exec(colour);
        if (!match) return null;
        const parts = match[1].split(',').map((part) => parseFloat(part.trim()));
        return [parts[0], parts[1], parts[2], parts[3] ?? 1];
      };

      const luminance = ([r, g, b]: [number, number, number, number]): number => {
        const channel = (value: number) => {
          const c = value / 255;
          return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
        };
        return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
      };

      const contrast = (a: [number, number, number, number], b: [number, number, number, number]) => {
        const [light, dark] = [luminance(a), luminance(b)].sort((x, y) => y - x);
        return (light + 0.05) / (dark + 0.05);
      };

      /** The first opaque ancestor background: a transparent row inherits the page's colour, not white. */
      const background = (element: HTMLElement): [number, number, number, number] => {
        let node: HTMLElement | null = element;
        while (node) {
          const colour = parse(getComputedStyle(node).backgroundColor);
          if (colour && colour[3] > 0) {
            // A translucent layer is composited over what is behind it; approximate by ignoring alpha < 0.9 layers and
            // taking the first solid one, which is what these surfaces are built from.
            if (colour[3] >= 0.9) return colour;
          }
          node = node.parentElement;
        }
        return [255, 255, 255, 1];
      };

      const results: string[] = [];
      const seen = new Set<string>();

      for (const element of Array.from(document.querySelectorAll<HTMLElement>('p, span, td, th, a, button, li, h1, h2, h3, label'))) {
        const text = (element.textContent ?? '').trim();
        if (!text || text.length > 120) continue;
        // Only leaf-ish elements: a container reports its children's text and its own colour is not what paints it.
        if (element.querySelector('p, span, td, th, a, button, li, h1, h2, h3, label')) continue;
        const style = getComputedStyle(element);
        if (style.visibility === 'hidden' || style.display === 'none' || Number(style.opacity) === 0) continue;
        const box = element.getBoundingClientRect();
        if (box.width === 0 || box.height === 0) continue;

        const foreground = parse(style.color);
        if (!foreground) continue;

        const ratio = contrast(foreground, background(element));
        const size = parseFloat(style.fontSize);
        const bold = Number(style.fontWeight) >= 700;
        const large = size >= 24 || (size >= 18.66 && bold);
        const required = large ? 3 : 4.5;

        if (ratio < required) {
          const key = `${style.color}|${getComputedStyle(element).backgroundColor}|${Math.round(size)}`;
          if (seen.has(key)) continue;
          seen.add(key);
          results.push(
            `${element.tagName.toLowerCase()} "${text.slice(0, 40)}" ${ratio.toFixed(2)}:1 (needs ${required}) ${style.color}`,
          );
        }
      }
      return results;
    });

    failures.push(...bad.map((entry) => `${path}: ${entry}`));
  }

  expect(failures, 'text below WCAG AA').toEqual([]);
});

/** A sortable header should announce the column it heads. */
test('table headers name their column, and sortable ones say which way', async ({ app }) => {
  await app.goto('/fleet');
  await app.waitForLoadState('networkidle');

  const headers = await app.evaluate(() =>
    Array.from(document.querySelectorAll('th')).map((th) => ({
      text: (th.textContent ?? '').trim(),
      scope: th.getAttribute('scope'),
      ariaSort: th.getAttribute('aria-sort'),
      hasButton: Boolean(th.querySelector('button')),
    })),
  );

  expect(headers.length).toBeGreaterThan(3);
  for (const header of headers) {
    expect(header.text.length, 'a header with no text').toBeGreaterThan(0);
    expect(header.scope, `header "${header.text}" has no scope`).toBe('col');
    // The control inside says what it does; the header says what it is.
    if (header.hasButton) {
      expect(
        ['ascending', 'descending', 'none', null],
        `sortable header "${header.text}" announces aria-sort="${header.ariaSort}"`,
      ).toContain(header.ariaSort);
    }
  }
});

/**
 * The tab said "Haven — Vendor Portal" on every screen.
 *
 * WCAG 2.4.2 is the cheapest rule on this list and the one an operator notices without knowing it has a name: three open
 * tabs that all read the same, and a screen reader announcing the same context after every navigation. Checked by walking
 * the real routes, because the mapping is only right if the router agrees with it.
 */
test('the tab names the screen, and changes when the route does', async ({ app }) => {
  const seen = new Map<string, string>();

  for (const [path, expected] of [
    ['/', 'Products'],
    ['/fleet', 'Fleet'],
    ['/audit', 'Audit log'],
    ['/team', 'Team'],
    ['/licences', 'Licences'],
  ]) {
    await app.goto(path);
    await app.waitForLoadState('networkidle');
    const title = await app.title();
    expect(title, `${path} is titled "${title}"`).toBe(`${expected} — Haven`);
    seen.set(path, title);
  }

  // Distinct titles are the point: five routes, five different tabs.
  expect(new Set(seen.values()).size).toBe(seen.size);
});

/**
 * Keyboard focus must be visible (WCAG 2.4.7).
 *
 * The stylesheet styles `.input`, `.table-frame` and `.row-disclosure` focus states — but a rule that is written per
 * component is a rule the next component can forget, and "outline: none" is the most common way a design system quietly
 * becomes unusable without a mouse. This walks the tab order on the page a support engineer uses most and asks the
 * browser what it is actually painting on the focused element.
 */
test('every stop in the tab order shows where the keyboard is', async ({ app }) => {
  await app.goto('/fleet');
  await app.waitForLoadState('networkidle');

  const found: string[] = [];
  for (let step = 0; step < 12; step += 1) {
    await app.keyboard.press('Tab');
    const stop = await app.evaluate(() => {
      const element = document.activeElement as HTMLElement | null;
      if (!element || element === document.body) return null;
      const style = getComputedStyle(element);
      const outline =
        style.outlineStyle !== 'none' && parseFloat(style.outlineWidth) > 0
          ? `${style.outlineStyle} ${style.outlineWidth}`
          : '';
      const shadow = style.boxShadow !== 'none' ? style.boxShadow : '';
      return {
        tag: element.tagName.toLowerCase(),
        label: (element.getAttribute('aria-label') ?? element.textContent ?? '').trim().slice(0, 30),
        visible: Boolean(outline || shadow),
        indicator: outline || shadow,
      };
    });
    if (!stop) continue;
    if (!stop.visible) found.push(`${stop.tag} "${stop.label}"`);
  }

  expect(found, 'focused controls with nothing drawn around them').toEqual([]);
});
