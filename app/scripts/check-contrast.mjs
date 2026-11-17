#!/usr/bin/env node
/**
 * Contrast gate for the app's palette.
 *
 * Every review of this app has found the same class of defect somewhere: a grey on a dark card, an
 * amber "Offline" that is legible on a phone held at arm's length and not on one held at a desk.
 * Vision review cannot settle it and a screenshot cannot prove it, so the palette is checked with
 * arithmetic instead, on every run.
 *
 * It reads `src/shared/config/tokens.ts` — the one file allowed to contain a hex literal — and fails
 * when a pair that ships as text drops under its threshold. Thresholds are WCAG 2.1: 4.5:1 for body
 * and small text, 3:1 for large text and for non-text UI (icons, borders that carry meaning).
 *
 * Run: node scripts/check-contrast.mjs [--json]
 */

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const source = readFileSync(join(root, 'src/shared/config/tokens.ts'), 'utf8');

/** Pull `accent: '#0A84FF'` style entries out of one `export const <name>` block. */
function paletteByName(name) {
  const start = source.indexOf(`export const ${name}: Palette = {`);
  if (start === -1) throw new Error(`palette ${name} not found in tokens.ts`);
  const end = source.indexOf('\n};', start);
  const block = source.slice(start, end);

  const values = {};
  for (const line of block.split('\n')) {
    const match = line.match(/^\s*([a-zA-Z]+):\s*'([^']+)'/);
    if (match) values[match[1]] = match[2];
  }
  return values;
}

/** `#RGB`, `#RRGGBB` and `rgba(r,g,b,a)` → { r, g, b, a } in 0–255 with alpha 0–1. */
function parse(colour) {
  const hex = colour.trim();
  if (hex.startsWith('#')) {
    const body = hex.slice(1);
    const full = body.length === 3 ? body.split('').map((c) => c + c).join('') : body;
    return {
      r: parseInt(full.slice(0, 2), 16),
      g: parseInt(full.slice(2, 4), 16),
      b: parseInt(full.slice(4, 6), 16),
      a: 1,
    };
  }
  const rgba = hex.match(/rgba?\(([^)]+)\)/);
  if (rgba) {
    const [r, g, b, a = '1'] = rgba[1].split(',').map((part) => part.trim());
    return { r: Number(r), g: Number(g), b: Number(b), a: Number(a) };
  }
  throw new Error(`cannot parse colour: ${colour}`);
}

/** A translucent colour as it actually appears on top of an opaque one. */
function over(front, back) {
  const f = parse(front);
  const b = parse(back);
  return {
    r: f.r * f.a + b.r * (1 - f.a),
    g: f.g * f.a + b.g * (1 - f.a),
    b: f.b * f.a + b.b * (1 - f.a),
    a: 1,
  };
}

function luminance({ r, g, b }) {
  const channel = (value) => {
    const v = value / 255;
    return v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
}

function ratio(foreground, background) {
  const fg = luminance(foreground);
  const bg = luminance(background);
  const [light, dark] = fg > bg ? [fg, bg] : [bg, fg];
  return (light + 0.05) / (dark + 0.05);
}

/**
 * What each palette is checked against — the pairs that actually ship as text or as meaning.
 *
 * `min` is where that pair is used: 4.5 for anything read as a sentence, 3 for a glyph or a status
 * dot, because a 6 pt dot carries no letters.
 */
const checks = (p) => {
  const bg = p.bg;
  const surface = p.surface;
  const glass = p.glass.startsWith('#') ? p.glass : null;
  const glassOnBg = glass ? over(glass, bg) : over(p.glass, bg);

  return [
    ['text on bg', over(p.text, bg), parse(bg), 4.5],
    ['text on surface', over(p.text, surface), parse(surface), 4.5],
    ['text on glass', over(p.text, bg), glassOnBg, 4.5],
    ['muted on bg', over(p.muted, bg), parse(bg), 4.5],
    ['muted on surface', over(p.muted, surface), parse(surface), 4.5],
    ['muted on glass', over(p.muted, bg), glassOnBg, 4.5],
    ['accent on bg', parse(p.accent), parse(bg), 3],
    ['accent on surface', parse(p.accent), parse(surface), 3],
    ['onAccent on accent', parse(p.onAccent), parse(p.accent), 4.5],
    ['success on bg', parse(p.success), parse(bg), 4.5],
    ['success on surface', parse(p.success), parse(surface), 4.5],
    ['warning on bg', parse(p.warning), parse(bg), 4.5],
    ['warning on surface', parse(p.warning), parse(surface), 4.5],
    ['danger on bg', parse(p.danger), parse(bg), 4.5],
    ['danger on surface', parse(p.danger), parse(surface), 4.5],
  ];
};

/**
 * Category accents are glyph and border colours, not sentences: 3:1 is the threshold, against the
 * surface they sit on. They are checked per appearance, because the palette deliberately has two.
 */
function categoryAccents(blockName) {
  const start = source.indexOf(`export const ${blockName}: Record<string, { accent: string; glow: string }>`);
  if (start === -1) return {};
  const end = source.indexOf('\n};', start);
  const block = source.slice(start, end);

  const values = {};
  for (const line of block.split('\n')) {
    const match = line.match(/^\s*([a-zA-Z]+):\s*\{\s*accent:\s*'([^']+)'/);
    if (match) values[match[1]] = match[2];
  }
  return values;
}

const json = process.argv.includes('--json');
const report = {};
let failures = 0;

for (const name of ['light', 'dark']) {
  const palette = paletteByName(name);
  const rows = checks(palette).map(([label, fg, bg, min]) => {
    const value = ratio(fg, bg);
    const pass = value + 1e-9 >= min;
    if (!pass) failures += 1;
    return { label, ratio: Number(value.toFixed(2)), min, pass };
  });
  // The category palette that ships with this appearance, against its own surface.
  const accents = categoryAccents(name === 'dark' ? 'categoryAccent' : 'categoryAccentLight');
  const theme = paletteByName(name);
  for (const [category, colour] of Object.entries(accents)) {
    const value = ratio(parse(colour), parse(theme.surface));
    const pass = value + 1e-9 >= 3;
    if (!pass) failures += 1;
    rows.push({ label: `category ${category}`, ratio: Number(value.toFixed(2)), min: 3, pass });
  }

  report[name] = rows;

  if (!json) {
    console.log(`\n${name}`);
    for (const row of rows) {
      const mark = row.pass ? 'ok  ' : 'FAIL';
      console.log(`  ${mark} ${row.label.padEnd(22)} ${String(row.ratio).padStart(5)} : 1  (min ${row.min})`);
    }
  }
}

if (json) console.log(JSON.stringify(report, null, 2));

if (failures > 0) {
  console.error(`\ncontrast: ${failures} pair${failures === 1 ? '' : 's'} below threshold`);
  process.exit(1);
}

if (!json) console.log('\ncontrast: every text pair meets its threshold');
