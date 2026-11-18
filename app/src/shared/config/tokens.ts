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
  /**
   * The accent at *text* weight. Apple's system colours are tints — they are chosen to be a filled button or a
   * glyph, not a sentence — and the same is true of this one: `accent` measured 4.08:1 as an 11-point label on
   * a translucent card, under AA, while being perfectly legible as a 28-point number or a switch. So the fill
   * keeps `accent` and the letters get `accentInk`.
   */
  accentInk: string;
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
  /** The operator's second colour: charts, secondary highlights, the second washes of the mesh. */
  accentAlt: string;
  /** The middle stop of the canvas gradient — without it the mesh falls off in a visible band. */
  bgMid: string;
  /** Where a wash fades to nothing. A wash that ends on its accent colour leaves a hard edge. */
  washEnd: string;
  /** The floating tab bar's surface: translucent in dark, near-solid in light. */
  chrome: string;
  /** Anything that casts: the tab bar, the raised centre action. Not a colour, a shadow. */
  shadow: string;
  /** A switch's knob. Needs to read on the track in both appearances. */
  thumb: string;
  /** A label on a filled danger button — white reads as an action, black as a warning. */
  dangerOn: string;
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
  // secondaryLabel at 72%, not Apple's 60%: at 60% it measures 3.3:1 on the grouped background,
  // which is below the 4.5:1 floor the contrast gate enforces. Slightly darker, still secondary.
  // 0.82 measured 4.06:1 where this text is really drawn — on the mesh, not on a white card. The same
  // lesson as dark mode's 0.60: the token is judged against the surface it lands on.
  muted: 'rgba(60,60,67,0.90)',
  // The system blue is 4.02:1 against white text — a button label nobody could pass AA with. This is
  // the same hue two steps deeper, 5.8:1.
  accent: '#0062CC',
  // 7.7:1 on white and 7.3:1 on the light card: the same hue, deep enough to be read as type.
  accentInk: '#0051A8',
  // A filled iOS button puts white on the tint, not black.
  onAccent: '#FFFFFF',
  // Apple's system green/amber/red are *tints*, not text: 1.99, 1.97 and 3.18:1 on the background.
  // These are the same signals at text weight (4.6–5.2:1), used for the offline pill and refusals.
  success: '#157F3C',
  warning: '#A15200',
  danger: '#C81E11',
  bgTop: '#F7F8FC',
  bgBottom: '#E8ECF6',
  // 0.78 of white over a pale mesh is a shape you have to look for. Light-mode glass has to hold its own edge;
  // dark-mode glass is a brightening, light-mode glass is a whiteness, and both are judged on the mesh.
  glass: 'rgba(255,255,255,0.92)',
  glow: 'rgba(0,122,255,0.14)',
  accentAlt: '#8B5CF6',
  bgMid: '#EDF1FA',
  washEnd: '#93A7C9',
  chrome: 'rgba(255,255,255,0.94)',
  shadow: 'rgba(15,23,42,0.18)',
  thumb: '#FFFFFF',
  dangerOn: '#FFFFFF',
};

/** iOS dark appearance. Not a dimmed light theme: different values by design. */
export const dark: Palette = {
  bg: '#000000',
  surface: '#1C1C1E',
  elevated: '#2C2C2E',
  border: 'rgba(84,84,88,0.65)',
  text: '#FFFFFF',
  // 0.72, not Apple's 0.6. Secondary label at 60 % white is 5.4:1 on the base background — and the contrast
  // gate passes it — but every card in this app is translucent over a coloured mesh, and measured from
  // rendered pixels an 11-point caption landed at 3.8–4.4:1 on the real surface. The palette has to answer
  // for the surface it is actually drawn on.
  muted: 'rgba(235,235,245,0.72)',
  // 4.59:1 against white (the system blue is 3.65) and still 4.1:1 on the night background, so a
  // filled button is readable without the accent disappearing into the canvas.
  accent: '#146EF5',
  // The dark accent is a fill: 4.08:1 as 11-point text on a card, 7.17:1 two steps lighter. Same hue, so the
  // brand still reads, and the small print still passes.
  accentInk: '#5AA2FF',
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
  accentAlt: '#8B5CF6',
  bgMid: '#080B18',
  washEnd: '#0E7490',
  chrome: 'rgba(11,18,32,0.82)',
  shadow: '#000000',
  thumb: '#FFFFFF',
  dangerOn: '#FFFFFF',
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
  /**
   * Room for the floating tab bar.
   *
   * It is absolute, over the content, so every scrollable screen has to end above it — and a screen
   * that guesses its own padding is the one where the last row hides under the bar. The bar is 68 pt
   * tall plus its gap and the home indicator; 148 pt clears all three.
   */
  layout: { tabBarHeight: 68, tabBarClearance: 148 },
  /**
   * Chart chrome: the two numbers at a chart's edges and the time labels under it.
   *
   * They belong to the palette because the operator owns the brand and because the light appearance is where
   * a hardcoded grey stops being legible; `chartMask` is the colour an alpha-only SVG mask is painted with.
   */
  chart: { axis: 'rgba(235,235,245,0.45)', mask: '#FFFFFF' },
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
/**
 * The same idea in daylight.
 *
 * The night accents are chosen to glow on near-black; on a white card the same cyan measures 1.6:1
 * and stops being an icon at all. These are the 600–700 weights of the same hues, which keep the
 * category recognisable and still clear 3:1 against the surface — the gate in `make contrast` checks
 * every one of them.
 */
export const categoryAccentLight: Record<string, { accent: string; glow: string }> = {
  light: { accent: '#7C3AED', glow: 'rgba(124,58,237,0.14)' },
  switch: { accent: '#0284C7', glow: 'rgba(2,132,199,0.14)' },
  plug: { accent: '#0284C7', glow: 'rgba(2,132,199,0.14)' },
  sensor: { accent: '#0E7490', glow: 'rgba(14,116,144,0.14)' },
  climate: { accent: '#C2410C', glow: 'rgba(194,65,12,0.14)' },
  cover: { accent: '#047857', glow: 'rgba(4,120,87,0.14)' },
  lock: { accent: '#BE185D', glow: 'rgba(190,24,93,0.14)' },
  camera: { accent: '#1D4ED8', glow: 'rgba(29,78,216,0.14)' },
  speaker: { accent: '#6D28D9', glow: 'rgba(109,40,217,0.14)' },
  vacuum: { accent: '#A16207', glow: 'rgba(161,98,7,0.14)' },
  energy: { accent: '#B45309', glow: 'rgba(180,83,9,0.14)' },
};

export function accentFor(
  category: string | undefined,
  fallback: string,
  dark = true,
): string {
  if (!category) return fallback;
  const map = dark ? categoryAccent : categoryAccentLight;
  return map[category.toLowerCase()]?.accent ?? fallback;
}

export function glowFor(category: string | undefined, fallback: string): string {
  if (!category) return fallback;
  return categoryAccent[category.toLowerCase()]?.glow ?? fallback;
}

export type Tokens = typeof tokens;
/** Who the product is, as the operator named it. */
export type Brand = { productName: string; logoText: string };

export type Theme = { color: Palette; dark: boolean; brand: Brand };

/**
 * The same palette, wearing an operator's accent.
 *
 * Only the three things that are *the brand* move: the accent, the text that sits on it, and the halo
 * derived from it. Everything else — backgrounds, text tiers, separators — stays the system's,
 * because a white-label customer is buying a product that still behaves like a phone, not a
 * colour picker that can make a screen unreadable.
 */
export function applyBrand(
  base: Palette,
  brand: { accent: string; accentOn?: string; accentSecondary?: string },
): Palette {
  const accent = brand.accent;
  return {
    ...base,
    accent,
    onAccent: brand.accentOn ?? base.onAccent,
    accentAlt: brand.accentSecondary ?? base.accentAlt,
    glow: withAlpha(accent, base.glow.includes('0.22') ? 0.22 : 0.14),
  };
}

/**
 * `#RRGGBB` + alpha, so one accent can tint a border, a halo and a chart fill.
 *
 * Lives here because this is the only file allowed to hold colour values: a helper that builds one
 * belongs to the same owner, and two copies of it would eventually disagree about a short hex.
 */
export function withAlpha(hex: string, alpha: number): string {
  const clean = hex.replace('#', '');
  if (clean.length !== 6) return hex;
  const r = parseInt(clean.slice(0, 2), 16);
  const g = parseInt(clean.slice(2, 4), 16);
  const b = parseInt(clean.slice(4, 6), 16);
  return `rgba(${r},${g},${b},${alpha})`;
}

/**
 * What kind of thing interrupted you, in a colour.
 *
 * Alerts are the one list where a glyph alone is not enough: "the doorbell saw someone" and "the
 * firmware failed" arrive in the same place, and the colour is what separates them before the title
 * is read. These are deliberately not palette accents — a security notice must not change colour
 * because an operator picked a different brand.
 */
export const signalAccent = {
  automation: '#8B5CF6',
  ota: '#0EA5E9',
  security: '#F472B6',
  alert: '#FB923C',
} as const;

export type SignalKind = keyof typeof signalAccent;
