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
  /** The canvas gradient. Two stops are enough to read as depth without a blur pass. */
  bgTop: string;
  bgBottom: string;
  /** A card that sits on the gradient: translucent, so the canvas shows through it. */
  glass: string;
  /** Accent at low alpha, for the halo behind an icon and the fill under a chart. */
  glow: string;
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
  bgTop: '#F7F8FC',
  bgBottom: '#E8ECF6',
  glass: 'rgba(255,255,255,0.78)',
  glow: 'rgba(0,122,255,0.14)',
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
  // A night sky rather than a black rectangle: the canvas is what makes a translucent card read as
  // glass instead of as a grey box, and it is the only thing on screen that is allowed to be quiet.
  bgTop: '#0B1020',
  bgBottom: '#05070F',
  glass: 'rgba(255,255,255,0.06)',
  glow: 'rgba(10,132,255,0.22)',
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

/**
 * One accent per category, so a bulb and a camera are told apart by colour before a word is read.
 *
 * This is the part that makes a *vendor's* product look designed without the vendor designing
 * anything: a product declares `category`, and the whole card — icon halo, control tint, chart fill —
 * takes that colour. An unknown category falls back to the palette accent rather than to grey, so a
 * new kind of device is never the one that looks broken.
 */
export const categoryAccent: Record<string, { accent: string; glow: string }> = {
  light: { accent: '#8B5CF6', glow: 'rgba(139,92,246,0.28)' },
  switch: { accent: '#38BDF8', glow: 'rgba(56,189,248,0.28)' },
  plug: { accent: '#38BDF8', glow: 'rgba(56,189,248,0.24)' },
  sensor: { accent: '#22D3EE', glow: 'rgba(34,211,238,0.24)' },
  climate: { accent: '#FB923C', glow: 'rgba(251,146,60,0.26)' },
  cover: { accent: '#34D399', glow: 'rgba(52,211,153,0.24)' },
  lock: { accent: '#F472B6', glow: 'rgba(244,114,182,0.26)' },
  camera: { accent: '#60A5FA', glow: 'rgba(96,165,250,0.26)' },
  speaker: { accent: '#A78BFA', glow: 'rgba(167,139,250,0.26)' },
  vacuum: { accent: '#FACC15', glow: 'rgba(250,204,21,0.22)' },
  energy: { accent: '#FBBF24', glow: 'rgba(251,191,36,0.24)' },
};

/** The accent for a category, or the theme's own when the category is one nobody planned for. */
export function accentFor(category: string | undefined, fallback: string): string {
  if (!category) return fallback;
  return categoryAccent[category.toLowerCase()]?.accent ?? fallback;
}

export function glowFor(category: string | undefined, fallback: string): string {
  if (!category) return fallback;
  return categoryAccent[category.toLowerCase()]?.glow ?? fallback;
}

export type Tokens = typeof tokens;
export type Theme = { color: Palette; dark: boolean };
