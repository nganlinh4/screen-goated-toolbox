#!/usr/bin/env node
/**
 * Exports all 12 cursors of a pack as a reference PNG sprite sheet.
 *
 * Usage (run from repo root):
 *   node screen-record/scripts/export_cursor_sprite.mjs --slug jepriwin11
 *   node screen-record/scripts/export_cursor_sprite.mjs --slug screenstudio --scale 3
 *   node screen-record/scripts/export_cursor_sprite.mjs --slug sgtcute --out ~/Desktop/ref.png
 *
 * Auto-installs @resvg/resvg-js on first run (no-save).
 */
import fs from 'node:fs';
import path from 'node:path';
import { execSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

const CURSOR_TYPES = [
  'default', 'text', 'pointer', 'openhand', 'closehand', 'wait',
  'appstarting', 'crosshair', 'resize-ns', 'resize-we', 'resize-nwse', 'resize-nesw',
];

const COLS      = 4;
const ROWS      = 3;
const SVG_W     = 44;   // cursor canvas width
const SVG_H     = 43;   // cursor canvas height
const LABEL_H   = 0;    // no label

function die(msg) {
  console.error(`[cursor-sprite] ${msg}`);
  process.exit(1);
}

function parseArgs(argv) {
  const args = { scale: 4 };
  for (let i = 2; i < argv.length; i++) {
    const t = argv[i];
    if      (t === '--slug')  args.slug  = argv[++i];
    else if (t === '--scale') args.scale = Number(argv[++i]);
    else if (t === '--out')   args.out   = argv[++i];
    else if (t === '--help' || t === '-h') {
      console.log('Usage: node screen-record/scripts/export_cursor_sprite.mjs --slug <pack> [--scale 4] [--out path.png]');
      process.exit(0);
    } else die(`Unknown arg: ${t}`);
  }
  if (!args.slug) die('Missing --slug');
  if (!Number.isFinite(args.scale) || args.scale <= 0) die('Invalid --scale');
  return args;
}

/** Prefix all id="..." and url(#...) references so multiple tiles don't clash. */
function scopeIds(svgText, prefix) {
  return svgText
    .replace(/\bid="([^"]+)"/g,  (_, id) => `id="${prefix}${id}"`)
    .replace(/url\(#([^)]+)\)/g, (_, id) => `url(#${prefix}${id})`);
}

/** Strip the outer <svg> open/close tags, keep inner markup only. */
function extractInner(svgText) {
  return svgText
    .replace(/^[\s\S]*?<svg\b[^>]*>\s*/i, '')
    .replace(/\s*<\/svg>\s*$/i, '');
}

function buildSpriteSvg(svgTexts, scale) {
  const tileW  = SVG_W * scale;
  const tileH  = SVG_H * scale;
  const cellH  = tileH + LABEL_H;
  const totalW = COLS * tileW;
  const totalH = ROWS * cellH;

  const lines = [
    `<svg xmlns="http://www.w3.org/2000/svg" width="${totalW}" height="${totalH}">`,
    `  <rect width="${totalW}" height="${totalH}" fill="#808080"/>`,
  ];

svgTexts.forEach((svgText, i) => {
    const col   = i % COLS;
    const row   = Math.floor(i / COLS);
    const x     = col * tileW;
    const y     = row * cellH;
    const inner = extractInner(scopeIds(svgText, `s${i}_`));


    // Cursor tile — embed inner content in a <svg> that maps 44×43 → tileW×tileH
    lines.push(
      `  <svg x="${x}" y="${y}" width="${tileW}" height="${tileH}" viewBox="0 0 ${SVG_W} ${SVG_H}" overflow="hidden">`,
      inner,
      `  </svg>`,
    );

  });

  lines.push('</svg>');
  return lines.join('\n');
}

async function main() {
  const args     = parseArgs(process.argv);
  const root     = process.cwd();
  const pubDir   = path.join(root, 'screen-record', 'public');
  const outPath  = args.out ?? path.resolve(`cursor-sprite-${args.slug}.png`);
  const installDir = path.join(__dirname, '..');  // screen-record/

  // Load the 12 cursor SVGs
  const svgTexts = CURSOR_TYPES.map((type) => {
    const fp = path.join(pubDir, `cursor-${type}-${args.slug}.svg`);
    if (!fs.existsSync(fp)) die(`Missing cursor file: ${fp}`);
    return fs.readFileSync(fp, 'utf8');
  });

  // Ensure @resvg/resvg-js is available
  const require = createRequire(import.meta.url);
  let Resvg;
  try {
    ({ Resvg } = require('@resvg/resvg-js'));
  } catch {
    console.log('[cursor-sprite] @resvg/resvg-js not found — installing (no-save)...');
    execSync('npm install --no-save @resvg/resvg-js', { stdio: 'inherit', cwd: installDir });
    ({ Resvg } = require('@resvg/resvg-js'));
  }

  const scale  = args.scale;
  const tileW  = SVG_W * scale;
  const tileH  = SVG_H * scale;
  const totalW = COLS * tileW;
  const totalH = ROWS * (tileH + LABEL_H);

  console.log(`[cursor-sprite] Rendering ${COLS}×${ROWS} grid at ${scale}x → ${totalW}×${totalH}px`);

  const svgStr = buildSpriteSvg(svgTexts, scale);
  const resvg  = new Resvg(svgStr, {
    fitTo: { mode: 'width', value: totalW },
    font:  { loadSystemFonts: true },
  });
  const png = resvg.render().asPng();

  fs.mkdirSync(path.dirname(outPath), { recursive: true });
  fs.writeFileSync(outPath, png);
  console.log(`[cursor-sprite] Saved: ${outPath}`);
}

main().catch((e) => { console.error(e); process.exit(1); });
