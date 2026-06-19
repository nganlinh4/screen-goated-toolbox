#!/usr/bin/env node
// Add a Material Symbols icon to screen-record's custom icon set
// (src/components/ui/MaterialIcon.tsx).
//
// The UI icon set is a hand-curated map of inline SVG path strings — there is
// no build-time icon pipeline. This script fetches the exact path data for a
// Material Symbols icon from the Iconify API and inserts both the ICON_PATHS
// entry and the matching `export const` line, alphabetically, so you don't have
// to paste path data by hand.
//
// Usage:
//   node screen-record/scripts/add_material_icon.mjs graphic_eq_off
//   node screen-record/scripts/add_material_icon.mjs screenshot-monitor
//   node screen-record/scripts/add_material_icon.mjs select_window --name SelectWindow
//
// The icon name is the Material Symbols name (snake_case or kebab-case), e.g.
// from https://fonts.google.com/icons. The export name defaults to its
// PascalCase form (graphic_eq_off -> GraphicEqOff); override with --name.

import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const HERE = dirname(fileURLToPath(import.meta.url));
const ICON_FILE = resolve(HERE, '../src/components/ui/MaterialIcon.tsx');

function toPascal(name) {
  return name
    .split(/[-_]/)
    .filter(Boolean)
    .map((p) => p.charAt(0).toUpperCase() + p.slice(1))
    .join('');
}

// First entry whose key sorts after `name` (plain lexicographic, matching the
// existing ordering in the file); -1 if `name` belongs at the end.
function findInsertIndex(lines, re, name) {
  let firstGreater = -1;
  let lastMatch = -1;
  for (let i = 0; i < lines.length; i++) {
    const m = lines[i].match(re);
    if (!m) continue;
    lastMatch = i;
    if (firstGreater === -1 && m[1] > name) firstGreater = i;
  }
  return firstGreater === -1 ? lastMatch + 1 : firstGreater;
}

async function main() {
  const args = process.argv.slice(2);
  const nameFlagIdx = args.indexOf('--name');
  const overrideName = nameFlagIdx >= 0 ? args[nameFlagIdx + 1] : null;
  const valueIdx = nameFlagIdx >= 0 ? nameFlagIdx + 1 : -1;
  const rawName = args.find((a, i) => !a.startsWith('--') && i !== valueIdx);

  if (!rawName) {
    console.error('Usage: node add_material_icon.mjs <material-symbol-name> [--name PascalName]');
    process.exit(1);
  }

  const pascal = overrideName ?? toPascal(rawName);
  const kebab = rawName.replace(/_/g, '-');

  const url = `https://api.iconify.design/material-symbols.json?icons=${kebab}`;
  const res = await fetch(url);
  if (!res.ok) {
    console.error(`Failed to fetch ${url}: HTTP ${res.status}`);
    process.exit(1);
  }
  const data = await res.json();
  if (data.not_found?.includes(kebab)) {
    console.error(`"${kebab}" not found in material-symbols. Check the name at https://fonts.google.com/icons`);
    process.exit(1);
  }
  const body = data.icons?.[kebab]?.body;
  if (!body) {
    console.error(`No icon body returned for "${kebab}".`);
    process.exit(1);
  }

  // The set only renders <path d="...">. Reject icons built from other
  // primitives (circle/rect/etc.) rather than silently dropping them.
  const paths = [...body.matchAll(/\bd="([^"]+)"/g)].map((m) => m[1]);
  const nonPath = body.replace(/<path\b[^>]*\/?>/g, '').replace(/<\/?(svg|g)\b[^>]*>/g, '').trim();
  if (paths.length === 0 || nonPath) {
    console.error(`"${kebab}" uses non-<path> primitives this set can't render; add it manually.`);
    process.exit(1);
  }

  let file = readFileSync(ICON_FILE, 'utf8');
  if (new RegExp(`^\\s*${pascal}:\\s*\\[`, 'm').test(file)) {
    console.log(`${pascal} already exists in MaterialIcon.tsx — nothing to do.`);
    return;
  }

  const entry = `  ${pascal}: [${paths.map((p) => `'${p}'`).join(', ')}],`;
  const exportLine = `export const ${pascal} = createMaterialIcon('${pascal}');`;

  const lines = file.split('\n');
  lines.splice(findInsertIndex(lines, /^ {2}(\w+): \[/, pascal), 0, entry);
  lines.splice(
    findInsertIndex(lines, /^export const (\w+) = createMaterialIcon\(/, pascal),
    0,
    exportLine,
  );

  writeFileSync(ICON_FILE, lines.join('\n'));
  console.log(`Added ${pascal} (${paths.length} path${paths.length > 1 ? 's' : ''}).`);
  console.log(`  import { ${pascal} } from '@/components/ui/MaterialIcon';`);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
