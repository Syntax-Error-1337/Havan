/**
 * Design tokens, in Apple's language rather than an invented one.
 *
 * Every value comes from the iOS Human Interface Guidelines:
 *
 * - **Type**: the SF text styles at the default Dynamic Type size — largeTitle
 *   34/41, title2 22/28, headline 17/22 semibold, body 17/22, footnote 13/18,
 *   caption2 11/13. Sizes are a starting point, not a cage: `Text` keeps
 *   `allowFontScaling`, so a person who raised the system text size gets larger
 *   text and the layout has to cope.
 * - **Colour**: system colours and the label hierarchy, one palette per
 *   appearance. `text` is full-strength label, `muted` is secondaryLabel at 60%,
 *   and the backgrounds are the grouped ones a List sits on.
 * - **Layout**: a 4 pt grid, 16 pt margins, and a 44 × 44 pt minimum hit target —
 *   the smallest target a finger hits reliably.
 *
 * Nothing outside this file may contain a hex literal: a white-label vendor
 * overrides `accent` at runtime and every screen follows.
 */

export type Palette = {
  bg: string;
  surface: string;
  elevated: string;
  border: string;
  text: string;
  muted: string;
  accent: string;
  onAccent: string;
  success: string;
  warning: string;
  danger: string;
};

/** iOS light appearance. */
export const light: Palette = {
  // systemGroupedBackground — what a grouped list sits on.
  bg: '#F2F2F7',
  // secondarySystemGroupedBackground — the inset cards themselves.
  surface: '#FFFFFF',
  // tertiarySystemGroupedBackground — a section strip.
  elevated: '#F2F2F7',
  // separator: black at 29%, the value UIKit draws hairlines with.
  border: 'rgba(60,60,67,0.29)',
  text: '#000000',
  // secondaryLabel: black at 60%.
  muted: 'rgba(60,60,67,0.6)',
  accent: '#007AFF',
  // A filled iOS button puts white on the tint, not black.
  onAccent: '#FFFFFF',
  success: '#34C759',
  warning: '#FF9500',
  danger: '#FF3B30',
};

/** iOS dark appearance. Not a dimmed light theme: different values by design. */
export const dark: Palette = {
  bg: '#000000',
  surface: '#1C1C1E',
  elevated: '#2C2C2E',
  border: 'rgba(84,84,88,0.65)',
  text: '#FFFFFF',
  muted: 'rgba(235,235,245,0.6)',
  accent: '#0A84FF',
  onAccent: '#FFFFFF',
  success: '#30D158',
  warning: '#FF9F0A',
  danger: '#FF453A',
};

export const tokens = {
  /** Dark is the default value of `tokens.color` for legacy call sites; the
   *  provider always supplies the real palette. */
  color: dark as Palette,
  /** The SF text styles this app uses, at default Dynamic Type. */
  font: {
    /** caption2 — the smallest thing that may appear outside a chart. */
    tiny: 11,
    /** footnote — secondary metadata next to a value. */
    caption: 13,
    /** subheadline — a dense row's second line. */
    subhead: 15,
    /** body — the default. */
    body: 17,
    /** title2 — a section title. */
    title: 22,
    /** largeTitle — one per screen, at the top. */
    display: 34,
  },
  lineHeight: { tiny: 13, caption: 18, subhead: 20, body: 22, title: 28, display: 41 },
  radius: { sm: 8, md: 10, lg: 12, xl: 16, pill: 999 },
  space: { xs: 4, sm: 8, md: 12, lg: 16, xl: 24, xxl: 32 },
  motion: { fast: 140, base: 220, slow: 380 },
  /** A finger, not a mouse: 44 pt is the smallest reliable target. */
  touch: { min: 44 },
} as const;

export type Tokens = typeof tokens;
export type Theme = { color: Palette; dark: boolean };
