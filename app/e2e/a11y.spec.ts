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

test('every screen names its controls, labels its fields and sizes its targets', async ({ page }) => {
  test.slow();
  await signIn(page);

  const screens: [string, string][] = [
    ['/', 'dashboard'],
    ['/settings', 'settings'],
    ['/automations', 'automations'],
    ['/scenes', 'scenes'],
    ['/alerts', 'alerts'],
    ['/homes', 'homes'],
    ['/privacy', 'privacy'],
  ];

  const findings: Finding[] = [];
  for (const [path, name] of screens) {
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
