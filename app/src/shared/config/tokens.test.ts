import { applyBrand, brandInk, contrastRatio, dark, light } from './tokens';

describe('brandInk', () => {
  it('leaves an accent alone when it is already readable as text', () => {
    // #5AA2FF on the dark background is 7.17:1 — the built-in ink, and the colour the app already uses.
    expect(brandInk('#5AA2FF', true)).toBe('#5AA2FF');
  });

  it('lightens a deep accent on a dark background until it can be read', () => {
    // Near-black navy: fine as a fill on a light card, invisible as a link on a dark one.
    const ink = brandInk('#1F2937', true);
    expect(contrastRatio(ink, dark.bg)).toBeGreaterThanOrEqual(4.5);
    expect(ink).not.toBe('#1F2937');
  });

  it('leaves the live brand accent alone, because it already passes on both appearances', () => {
    for (const mode of [true, false]) {
      const ink = brandInk('#146EF5', mode);
      expect(contrastRatio(ink, mode ? dark.bg : light.bg)).toBeGreaterThanOrEqual(4.5);
    }
  });

  it('darkens a pale accent on a light background until it can be read', () => {
    const ink = brandInk('#FDE047', false);
    expect(contrastRatio(ink, light.bg)).toBeGreaterThanOrEqual(4.5);
    expect(ink).not.toBe('#FDE047');
  });

  it('keeps the hue: a purple stays the most-red-and-most-blue channel', () => {
    const ink = brandInk('#8B5CF6', false).replace('#', '');
    const [r, g, b] = [0, 2, 4].map((i) => parseInt(ink.slice(i, i + 2), 16));
    expect(r).toBeGreaterThan(g!);
    expect(b).toBeGreaterThan(g!);
  });

  it('gives back a malformed colour untouched rather than a wrong one', () => {
    expect(brandInk('not-a-colour', true)).toBe('not-a-colour');
  });
});

describe('applyBrand', () => {
  it('moves the ink with the fill — the defect a white-label operator would have hit first', () => {
    // A green brand used to render green buttons with the built-in blue links, because `accentInk` was the one
    // brand colour that never moved.
    const branded = applyBrand(light, { accent: '#157F3C', accentSecondary: '#22D3EE' });
    expect(branded.accent).toBe('#157F3C');
    expect(branded.accentInk).not.toBe(light.accentInk);
    expect(contrastRatio(branded.accentInk, light.bg)).toBeGreaterThanOrEqual(4.5);
  });

  it('does not touch the surfaces: a brand is not a licence to repaint the phone', () => {
    const branded = applyBrand(dark, { accent: '#157F3C' });
    expect(branded.bg).toBe(dark.bg);
    expect(branded.text).toBe(dark.text);
    expect(branded.muted).toBe(dark.muted);
  });
});
