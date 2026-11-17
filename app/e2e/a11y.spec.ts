import { expect, test, type Page } from '@playwright/test';

/**
 * Accessibility, measured rather than asserted.
 *
 * The design system has a contrast gate, a token gate and a screenshot review; what it never had was a check
 * on the things a person who cannot see the screen depends on: a name for every control, a label for every
 * field, and a target big enough to hit.
 *
 * No axe: `axe-core` is MPL-2.0 and ADR-002 permits Apache/MIT/BSD/ISC/PostgreSQL/0BSD only. Three rules
 * written by hand cover what this app actually gets wrong — and, unlike a rule engine's 400 checks, each one
 * here is a claim somebody can argue with.
 */

const DEMO = {
  email: process.env.E2E_DEMO_EMAIL ?? 'demo@haven.local',
  password: process.env.E2E_DEMO_PASSWORD ?? 'haven12345',
};

/** Apple's minimum: 44 pt. The web build renders CSS pixels 1:1 on a phone viewport. */
const MIN_TARGET = 44;
/** A 1-px rounding allowance: a 44-px box with a hairline border measures 43.99. */
const TOLERANCE = 1;

type Finding = { screen: string; what: string; detail: string };

async function signIn(page: Page) {
  await page.goto('/');
  await page.getByLabel('Email', { exact: true }).fill(DEMO.email);
  await page.getByLabel('Password', { exact: true }).fill(DEMO.password);
  await page.getByRole('button', { name: 'Sign in', exact: true }).click();
  await expect(page.getByText('DASHBOARD')).toBeVisible();
}

/**
 * Controls without an accessible name.
 *
 * `name` is what a screen reader announces and what a voice-control user says out loud; an icon button with
 * none is a button that cannot be pressed by anybody who is not looking at it.
 */
async function unnamedControls(page: Page, screen: string): Promise<Finding[]> {
  const found = await page.evaluate(() => {
    const nameOf = (el: Element) =>
      (
        el.getAttribute('aria-label') ||
        el.getAttribute('aria-labelledby') ||
        (el as HTMLElement).innerText ||
        el.getAttribute('title') ||
        ''
      ).trim();

    const controls = Array.from(
      document.querySelectorAll('[role="button"], button, [role="switch"], [role="link"], a[href]'),
    );
    return controls
      .filter((el) => {
        const box = el.getBoundingClientRect();
        // Off-screen controls (a hidden sheet, a scrolled-out row) are not this screen's problem.
        return box.width > 0 && box.height > 0 && nameOf(el).length === 0;
      })
      .map((el) => el.outerHTML.slice(0, 120));
  });
  return found.map((detail) => ({ screen, what: 'control with no accessible name', detail }));
}

/** Fields without a label a screen reader can read. */
async function unlabelledFields(page: Page, screen: string): Promise<Finding[]> {
  const found = await page.evaluate(() => {
    const inputs = Array.from(document.querySelectorAll('input, textarea'));
    return inputs
      .filter((el) => {
        const box = el.getBoundingClientRect();
        if (box.width === 0 || box.height === 0) return false;
        const labelled =
          el.getAttribute('aria-label') ||
          el.getAttribute('aria-labelledby') ||
          el.getAttribute('placeholder') ||
          (el.id && document.querySelector(`label[for="${el.id}"]`));
        return !labelled;
      })
      .map((el) => el.outerHTML.slice(0, 120));
  });
  return found.map((detail) => ({ screen, what: 'input with no label', detail }));
}

/** Targets too small to hit reliably with a thumb. */
async function smallTargets(page: Page, screen: string): Promise<Finding[]> {
  const found = await page.evaluate((min) => {
    // `role="switch"` is deliberately excluded: iOS's own switch is 51 x 31, and enlarging the visual
    // shape to 44 would make it look like a button rather than a switch. The platform control is the
    // platform control; a bespoke button is not.
    const controls = Array.from(
      document.querySelectorAll('[role="button"], button, [role="link"]'),
    );
    return controls
      .filter((el) => {
        const box = el.getBoundingClientRect();
        if (box.width === 0 || box.height === 0) return false;
        // A hit-slop wrapper or a larger pressable parent makes a small glyph pressable; measure what the
        // finger actually lands on by walking up while the parent is still pressable.
        const parent = el.parentElement;
        const parentBox = parent?.getBoundingClientRect();
        const parentIsControl = parent?.getAttribute('role') === 'button';
        const height = parentIsControl && parentBox ? Math.max(box.height, parentBox.height) : box.height;
        const width = parentIsControl && parentBox ? Math.max(box.width, parentBox.width) : box.width;
        return height < min || width < min;
      })
      .map((el) => {
        const box = el.getBoundingClientRect();
        const label = (el.getAttribute('aria-label') || (el as HTMLElement).innerText || '?').trim();
        return `${label.slice(0, 40)} — ${Math.round(box.width)}×${Math.round(box.height)}`;
      });
  }, MIN_TARGET - TOLERANCE);
  return found.map((detail) => ({ screen, what: `target smaller than ${MIN_TARGET}px`, detail }));
}

async function audit(page: Page, screen: string): Promise<Finding[]> {
  return [
    ...(await unnamedControls(page, screen)),
    ...(await unlabelledFields(page, screen)),
    ...(await smallTargets(page, screen)),
  ];
}

const SCREENS: [string, string][] = [
  ['/', 'dashboard'],
  ['/settings', 'settings'],
  ['/automations', 'automations'],
  ['/scenes', 'scenes'],
  ['/alerts', 'alerts'],
  ['/homes', 'homes'],
  ['/privacy', 'privacy'],
  ['/add-device', 'add-device'],
  ['/device/01a1fb00-0000-7000-8000-000000000002', 'device detail'],
];

test('every screen names its controls, labels its fields and sizes its targets', async ({ page }) => {
  test.slow();
  await signIn(page);

  const findings: Finding[] = [];
  for (const [path, name] of SCREENS) {
    await page.goto(path);
    await page.waitForLoadState('networkidle');
    findings.push(...(await audit(page, name)));
  }

  // One assertion for the whole sweep, printed as a table: a failure should say what to fix, not just which
  // screen to open.
  expect(
    findings.map((f) => `${f.screen}: ${f.what} — ${f.detail}`).join('\n'),
    'accessibility findings',
  ).toBe('');
});

/**
 * The light palette is a different set of colours, and a contrast gate that only ever runs against dark is a
 * gate with half a product behind it. The same three rules, with the customer's own appearance set.
 */
test('the same rules hold in the appearance the customer chose', async ({ page }) => {
  test.slow();
  await signIn(page);

  // Set the preference the way the app does, then reload so the provider reads it fresh.
  await page.evaluate(async () => {
    const existing = Number(localStorage.getItem('haven.appearance.rev') ?? '0');
    localStorage.setItem('haven.appearance', 'light');
    localStorage.setItem('haven.appearance.rev', String(existing + 1));
  });
  await page.reload();
  await expect(page.getByText('DASHBOARD')).toBeVisible();

  const findings: Finding[] = [];
  for (const [path, name] of SCREENS) {
    await page.goto(path);
    await page.waitForLoadState('networkidle');
    findings.push(...(await audit(page, `light ${name}`)));
  }

  // Put the default back for whatever runs next.
  await page.evaluate(() => localStorage.setItem('haven.appearance', 'system'));

  expect(
    findings.map((f) => `${f.screen}: ${f.what} — ${f.detail}`).join('\n'),
    'accessibility findings in light mode',
  ).toBe('');
});

/**
 * 200% text zoom, which iOS and Android both allow and which the web build gets for free from browser zoom.
 *
 * A 390-point phone at 200% sees 195 points of layout. That is emulated by halving the viewport rather than
 * multiplying the fonts: the property under test is "does anything fall off the side of the screen", and a
 * narrower viewport asks it exactly.
 */
test('nothing falls off the side of the screen at 200% zoom', async ({ page }) => {
  test.slow();
  await signIn(page);
  await page.setViewportSize({ width: 195, height: 422 });

  const findings: string[] = [];
  for (const [path, name] of SCREENS) {
    await page.goto(path);
    await page.waitForLoadState('networkidle');
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
    expect(overflow, `${name} scrolls sideways at 200% zoom`).toBeLessThanOrEqual(1);
    // A control pushed past the right edge is unreachable even when the page does not scroll: the app is a
    // fixed-width canvas, so anything wider than it is simply gone.
    const offscreen = await page.evaluate(() => {
      // A control inside a horizontal ScrollView is *meant* to be past the edge — the row scrolls to it.
      // Only a control that is past the edge with nothing to scroll counts as unreachable.
      const scrolls = (el: Element) => {
        for (let node = el.parentElement; node; node = node.parentElement) {
          const style = getComputedStyle(node);
          if ((style.overflowX === 'auto' || style.overflowX === 'scroll') && node.scrollWidth > node.clientWidth) {
            return true;
          }
        }
        return false;
      };
      const controls = Array.from(document.querySelectorAll('[role="button"], button, input'));
      return controls
        .filter((el) => {
          const box = el.getBoundingClientRect();
          if (box.width === 0 || box.right <= window.innerWidth + 2) return false;
          return !scrolls(el);
        })
        .map((el) => (el.getAttribute('aria-label') || (el as HTMLElement).innerText || '?').slice(0, 30));
    });
    findings.push(...offscreen.map((label) => `${name}: "${label}" is past the right edge`));
  }
  expect(findings.join('\n'), 'controls off screen at 200% zoom').toBe('');
});
