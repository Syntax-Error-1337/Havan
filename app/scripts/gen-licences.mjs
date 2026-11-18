#!/usr/bin/env node
/**
 * The app's dependency licences, as data the app can show.
 *
 * Every shipping app has to be able to answer "what is in this binary and under what licence" — App Store review asks,
 * enterprise buyers ask, and AGENTS.md already treats the licence rule as non-negotiable. The alternative is a
 * hand-maintained list that is wrong within a month.
 *
 * Reads the installed tree rather than package.json alone: a dependency's `license` field is often in its own manifest
 * and nowhere else, and an entry marked "NOASSERTION" is a dependency nobody has checked.
 *
 *   node scripts/gen-licences.mjs
 */
import { readFileSync, writeFileSync, readdirSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, '..');
const pkg = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8'));
const deps = { ...pkg.dependencies };

const rows = [];
const missing = [];

for (const [name, range] of Object.entries(deps)) {
  let manifest;
  try {
    manifest = JSON.parse(readFileSync(join(root, 'node_modules', name, 'package.json'), 'utf8'));
  } catch {
    missing.push(name);
    continue;
  }
  const license =
    typeof manifest.license === 'string'
      ? manifest.license
      : (manifest.licenses ?? []).map((l) => (typeof l === 'string' ? l : l.type)).join(' OR ') || 'NOASSERTION';
  rows.push({ name, version: manifest.version ?? range, license });
}

rows.sort((a, b) => a.name.localeCompare(b.name));

const out = join(root, 'src', 'shared', 'config', 'licences.json');
writeFileSync(out, JSON.stringify({ generated: new Date().toISOString().slice(0, 10), packages: rows }, null, 2) + '\n');

// A count, never a dump: this runs in CI, and a 400-line wall of JSON is scrolled past.
const byLicense = rows.reduce((acc, row) => ({ ...acc, [row.license]: (acc[row.license] ?? 0) + 1 }), {});
console.log(`licences: ${rows.length} packages → ${Object.entries(byLicense).map(([k, v]) => `${k} ${v}`).join(', ')}`);
if (missing.length) {
  console.error(`licences: ${missing.length} not installed: ${missing.join(', ')}`);
  process.exitCode = 1;
}
