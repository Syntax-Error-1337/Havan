#!/usr/bin/env node
/**
 * The token gate: no colour value may live outside `src/shared/config/tokens.ts`.
 *
 * The contrast gate proves the palette is readable. This one proves the palette is the *only* place a
 * colour is decided — because the moment a screen hardcodes `rgba(255,255,255,0.16)` for a dashed
 * border, an operator who picks the light appearance gets a border nobody can see, and no contrast
 * check will ever look at it. That is not a hypothetical: it is what the alerts and scene-strip
 * screens shipped with until this gate existed.
 *
 * It reads source text, so it strips comments first — a comment may discuss `rgba()` without
 * declaring one.
 *
 *   node scripts/check-tokens.mjs          # human output, exit 1 on any literal
 *   node scripts/check-tokens.mjs --json   # machine output for CI
 */

import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = fileURLToPath(new URL('..', import.meta.url));
const SRC = join(ROOT, 'src');
/** The one file allowed to hold colour values. */
const OWNER = join(SRC, 'shared', 'config', 'tokens.ts');

const SKIP_DIRS = new Set(['node_modules', '.expo', 'dist', 'build']);

/** Hex colours and functional colour notation. Short hex is included: `#fff` is a decision too. */
const PATTERNS = [
  { name: 'hex', re: /#(?:[0-9a-fA-F]{8}|[0-9a-fA-F]{6}|[0-9a-fA-F]{3,4})\b/g },
  { name: 'functional', re: /\brgba?\s*\(/g },
  { name: 'hsl', re: /\bhsla?\s*\(/g },
];

function walk(dir) {
  const out = [];
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (SKIP_DIRS.has(entry)) continue;
    const info = statSync(full);
    if (info.isDirectory()) out.push(...walk(full));
    else if (/\.(ts|tsx)$/.test(entry) && !/\.test\.(ts|tsx)$/.test(entry)) out.push(full);
  }
  return out;
}

/** Comments stripped, line numbers preserved, so a report can still point at a line. */
function stripComments(text) {
  const blank = (s) => s.replace(/[^\n]/g, ' ');
  let out = text.replace(/\/\*[\s\S]*?\*\//g, blank);
  out = out.replace(/(^|[^:'"\\])\/\/[^\n]*/g, (match, prefix) => prefix + blank(match.slice(prefix.length)));
  return out;
}

const findings = [];

for (const file of walk(SRC)) {
  if (file === OWNER) continue;
  const text = stripComments(readFileSync(file, 'utf8'));
  const lines = text.split('\n');

  lines.forEach((line, index) => {
    for (const { name, re } of PATTERNS) {
      re.lastIndex = 0;
      const match = re.exec(line);
      if (!match) continue;
      findings.push({
        file: relative(ROOT, file),
        line: index + 1,
        kind: name,
        literal: match[0].trim(),
      });
    }
  });
}

if (process.argv.includes('--json')) {
  console.log(JSON.stringify({ findings, total: findings.length }, null, 2));
  process.exit(findings.length === 0 ? 0 : 1);
}

if (findings.length === 0) {
  console.log('tokens: every colour in the app comes from shared/config/tokens.ts');
  process.exit(0);
}

console.error(`tokens: ${findings.length} colour literal(s) outside shared/config/tokens.ts\n`);
for (const f of findings) {
  console.error(`  ${f.file}:${f.line}  ${f.literal}`);
}
console.error(
  '\nAdd a named token to src/shared/config/tokens.ts and use it. A screen that hardcodes a colour\n' +
    'overrides the operator, who owns the brand — and the light appearance is the one that finds out.',
);
process.exit(1);
