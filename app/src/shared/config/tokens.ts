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
   * Hint text inside an empty field.
   *
   * Lighter than `muted` on purpose: at 0.90 the scene editor's placeholder was indistinguishable from typed
   * text, so an empty form looked like a filled one. Safe to lighten here because every field that uses it also
   * carries a persistent label — the placeholder is an example, not the field's only name.
   */
  placeholder: string;
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
  /**
   * A content surface.
   *
   * Not `glass`. Apple's rule is blunt — `materials.md › Liquid Glass`: "Don't use Liquid Glass in the content
   * layer … it can result in unnecessary complexity and a confusing visual hierarchy" — and this app had glass on
   * every card, which is the defect the rule names. A card is a *material*: near-opaque, lit from above, one
   * shadow. Glass is rationed to the functional layer (the tab bar, the sheets, the floating room capsule) where
   * something colourful actually scrolls beneath it and the blur has a job.
   */
  card: string;
  /** The 1 pt highlight along a card's top edge. A surface with a light source reads deeper than a flat one. */
  cardEdge: string;
  /** Behind white text that sits on a category-tinted art field. Never a text shadow: that fails on light fields. */
  scrim: string;
  /** A gauge's or slider's unfilled rail. */
  track: string;
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
  placeholder: 'rgba(60,60,67,0.42)',
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
  // 96 %, not 100 %: a card still has to sit *on* the canvas for the lit edge and the shadow to read.
  card: 'rgba(255,255,255,0.96)',
  cardEdge: 'rgba(255,255,255,0.90)',
  scrim: 'rgba(8,10,18,0.38)',
  track: 'rgba(120,120,128,0.16)',
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
  placeholder: 'rgba(235,235,245,0.34)',
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
  card: 'rgba(20,24,33,0.88)',
  cardEdge: 'rgba(255,255,255,0.10)',
  scrim: 'rgba(0,0,0,0.42)',
  track: 'rgba(120,120,128,0.32)',
};

export const tokens = {
  /**
   * The palette the app boots into, before the provider exists.
   *
   * This used to be `tokens.color`, described as "the default for legacy call sites" — and three screens
   * believed it: the scenes editor, the automations editor and the radar handoff each drew their text fields
   * from it, so in the light appearance they were black boxes on white cards. A colour that is not from
   * `useTheme()` is a dark-mode assumption wearing a token's clothes, so the misleading name is gone: only the
   * pre-hydration splash may use `bootPalette`, and `check-tokens.mjs` enforces that.
   */
  bootPalette: dark as Palette,
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
    /**
     * The reading. `typography.md › Using system fonts` asks for hierarchy from size and weight rather than from a
     * second family, and a device's number is the reason the card exists — it had been drawn at body size under a
     * device name, which is the wrong way round.
     */
    numeral: 34,
    /** The same idea at the top of the hero, inside the gauge. Ultra-light: at 44 pt a hairline weight still reads. */
    gauge: 44,
  },
  lineHeight: { tiny: 13, caption: 18, subhead: 20, body: 22, title: 28, display: 41, numeral: 38, gauge: 46 },
  radius: {
    sm: 8,
    md: 10,
    lg: 12,
    xl: 16,
    /** A card. 22 on a 168 pt tile reads as a soft product; 16 read as a web card. */
    card: 22,
    /** The art field inside a card, and inner controls that are not pills. */
    field: 12,
    pill: 999,
  },
  space: { xs: 4, sm: 8, md: 12, lg: 16, xl: 24, xxl: 32 },
  /**
   * One light source, three heights.
   *
   * Offsets and blur only; the colour comes from the palette's `shadow` at render time, and `shared/ui/Card` turns
   * these into `shadowOffset/shadowRadius/shadowOpacity` on iOS, `elevation` on Android and `boxShadow` on web —
   * three properties that must agree, which is exactly the drift one helper prevents.
   */
  elevation: {
    1: { y: 1, blur: 2, opacity: 0.06 },
    2: { y: 8, blur: 24, opacity: 0.1 },
    3: { y: 12, blur: 32, opacity: 0.18 },
  },
  motion: { fast: 140, base: 220, slow: 380, gauge: 380 },
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
    // The ink as well as the fill. It was left at the *built-in* value, so a white-label operator who chose
    // green got green buttons and blue links: every `tone="accent"` label in the app is drawn in `accentInk`,
    // and it was the one brand colour that never moved.
    accentInk: brandInk(accent, base === dark),
    onAccent: brand.accentOn ?? base.onAccent,
    accentAlt: brand.accentSecondary ?? base.accentAlt,
    glow: withAlpha(accent, base.glow.includes('0.22') ? 0.22 : 0.14),
  };
}

/** Relative luminance, WCAG 2.1. */
function luminance(hex: string): number {
  const clean = hex.replace('#', '');
  if (clean.length !== 6) return 0;
  const channel = (i: number) => {
    const c = parseInt(clean.slice(i, i + 2), 16) / 255;
    return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(0) + 0.7152 * channel(2) + 0.0722 * channel(4);
}

/** Contrast between two colours, 1:1 … 21:1. */
export function contrastRatio(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x) as [number, number];
  return (hi + 0.05) / (lo + 0.05);
}

/**
 * The brand accent, adjusted until it can be used as **text** on this appearance.
 *
 * A fill and a label are different jobs: `#8B5CF6` is a fine button and a 3.2:1 caption. The operator picks one
 * colour; this walks it lighter (on a dark background) or darker (on a light one) in small steps, keeping the
 * hue, until it clears 4.5:1 — which is the same threshold `check-contrast.mjs` enforces, applied to a colour
 * that arrives at runtime and cannot be reviewed.
 */
export function brandInk(accent: string, darkMode: boolean): string {
  const background = darkMode ? dark.bg : light.bg;
  const clean = accent.replace('#', '');
  if (clean.length !== 6) return accent;

  const toRgb = () => [0, 2, 4].map((i) => parseInt(clean.slice(i, i + 2), 16));
  const hex = (channels: number[]) =>
    `#${channels.map((c) => Math.round(c).toString(16).padStart(2, '0')).join('')}`.toUpperCase();

  let channels = toRgb();
  let candidate = hex(channels);
  // 20 steps of 8 % is enough to walk the darkest blue to a legible tint and the lightest yellow to a legible
  // shade; the loop stops as soon as the ratio clears, so a good accent is returned unchanged.
  for (let step = 0; step < 20 && contrastRatio(candidate, background) < 4.5; step += 1) {
    channels = channels.map((c) => (darkMode ? c + (255 - c) * 0.12 : c * 0.88));
    candidate = hex(channels);
  }
  return candidate;
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
