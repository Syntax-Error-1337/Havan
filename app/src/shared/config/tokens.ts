/**
 * The single source of colour, spacing and motion.
 *
 * Nothing outside this file may contain a hex literal. White-label vendors
 * override `accent` and `bg` at runtime from `/vendor/branding`; every primitive
 * reads the theme, so one object repaints the app.
 */

export const tokens = {
  color: {
    bg: '#0B0D10',
    surface: '#14181D',
    elevated: '#1B2027',
    border: '#252C35',
    text: '#F2F5F8',
    muted: '#8A97A6',
    accent: '#4C8DFF',
    onAccent: '#04070C',
    success: '#2FBF71',
    warning: '#F2A33C',
    danger: '#E5484D',
  },
  radius: { sm: 8, md: 12, lg: 18, pill: 999 },
  space: { xs: 4, sm: 8, md: 12, lg: 16, xl: 24, xxl: 32 },
  font: { caption: 12, body: 15, title: 20, display: 28 },
  motion: { fast: 140, base: 220, slow: 380 },
  touch: { min: 44 },
} as const;

export type Tokens = typeof tokens;
export type Theme = { color: Tokens['color']; dark: boolean };
