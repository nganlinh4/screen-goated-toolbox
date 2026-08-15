#!/usr/bin/env node
// Download a Material Symbols Rounded glyph and register it with the native
// egui icon atlas. SVG path data must come from this command, not be authored
// or pasted by hand.
//
// Usage: node scripts/add_egui_material_icon.mjs drag_indicator DragIndicator

import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const ICON_DIR = resolve(ROOT, 'src/gui/icons');
const MANIFEST_PATH = resolve(ICON_DIR, 'manifest.json');
const ICON_MODULE_PATH = resolve(ICON_DIR, 'mod.rs');

function fail(message) {
  console.error(message);
  process.exit(1);
}

const [rawName, variant] = process.argv.slice(2);
if (!rawName || !variant || !/^[A-Z][A-Za-z0-9]*$/.test(variant)) {
  fail('Usage: node scripts/add_egui_material_icon.mjs <material-symbol-name> <RustIconVariant>');
}

const snakeName = rawName.replaceAll('-', '_');
if (!/^[a-z0-9_]+$/.test(snakeName)) {
  fail(`Invalid Material Symbol name: ${rawName}`);
}

const kebabName = snakeName.replaceAll('_', '-');
const sourceUrl = `https://api.iconify.design/material-symbols:${kebabName}-rounded.svg`;
const response = await fetch(sourceUrl);
if (!response.ok) {
  fail(`Failed to download ${sourceUrl}: HTTP ${response.status}`);
}
const downloadedSvg = (await response.text()).trim();
const svg = downloadedSvg.replaceAll('currentColor', '#ffffff');
if (!svg.startsWith('<svg') || !svg.includes('<path')) {
  fail(`Downloaded ${rawName} is not a path-based SVG`);
}
const iconModule = readFileSync(ICON_MODULE_PATH, 'utf8');
if (!new RegExp(`^\\s*${variant},?(?:\\s*//.*)?$`, 'm').test(iconModule)) {
  fail(`Add the ${variant} variant to src/gui/icons/mod.rs before registering its glyph`);
}

const manifest = JSON.parse(readFileSync(MANIFEST_PATH, 'utf8'));
if (!Array.isArray(manifest.sprites)) {
  fail('Native icon manifest has no sprites array');
}
for (const sprite of manifest.sprites) {
  if (sprite.variants.includes(variant) && sprite.file !== `${snakeName}.svg`) {
    fail(`${variant} is already registered by ${sprite.file}`);
  }
}

const file = `${snakeName}.svg`;
let entry = manifest.sprites.find((sprite) => sprite.file === file);
if (entry) {
  if (!entry.variants.includes(variant)) entry.variants.push(variant);
  entry.variants.sort();
} else {
  entry = { file, variants: [variant] };
  manifest.sprites.push(entry);
}
manifest.sprites.sort((left, right) =>
  left.file < right.file ? -1 : left.file > right.file ? 1 : 0,
);

const manifestLines = manifest.sprites.map((sprite) => {
  const variants = sprite.variants.map((name) => JSON.stringify(name)).join(', ');
  return `    { "file": ${JSON.stringify(sprite.file)}, "variants": [${variants}] }`;
});
const formattedManifest = `{\n  "sprites": [\n${manifestLines.join(',\n')}\n  ]\n}\n`;

writeFileSync(resolve(ICON_DIR, 'svg', file), `${svg}\n`);
writeFileSync(MANIFEST_PATH, formattedManifest);
console.log(`Downloaded Material Symbols Rounded '${rawName}' and registered Icon::${variant}.`);
