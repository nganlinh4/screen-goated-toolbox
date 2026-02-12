#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';

const CURSOR_TYPES = [
  'default',
  'text',
  'pointer',
  'openhand',
  'closehand',
  'wait',
  'appstarting',
  'crosshair',
  'resize-ns',
  'resize-we',
  'resize-nwse',
  'resize-nesw',
];

const OUTER_W = 44;
const OUTER_H = 43;
const INNER_W = 38.28;
const INNER_H = 37.41;
const INNER_X = (OUTER_W - INNER_W) / 2;
const INNER_Y = (OUTER_H - INNER_H) / 2;

function die(msg) {
  console.error(`[cursor-pack] ${msg}`);
  process.exit(1);
}

function parseArgs(argv) {
  const args = { columns: 4, rows: 3 };
  for (let i = 2; i < argv.length; i += 1) {
    const token = argv[i];
    if (token === '--input') {
      args.input = argv[++i];
    } else if (token === '--slug') {
      args.slug = argv[++i];
    } else if (token === '--columns') {
      args.columns = Number(argv[++i]);
    } else if (token === '--rows') {
      args.rows = Number(argv[++i]);
    } else if (token === '--help' || token === '-h') {
      console.log('Usage: node scripts/generate_cursor_pack.mjs --input <spritesheet.svg> --slug <packslug> [--columns 4 --rows 3]');
      process.exit(0);
    } else {
      die(`Unknown arg: ${token}`);
    }
  }
  if (!args.input) die('Missing --input');
  if (!args.slug) die('Missing --slug');
  if (!Number.isFinite(args.columns) || args.columns < 1) die('Invalid --columns');
  if (!Number.isFinite(args.rows) || args.rows < 1) die('Invalid --rows');
  if (args.columns * args.rows < CURSOR_TYPES.length) {
    die(`Grid ${args.columns}x${args.rows} has fewer slots than ${CURSOR_TYPES.length}`);
  }
  return args;
}

function parseRootSvg(svgText) {
  const rootMatch = svgText.match(/<svg\b[^>]*>/i);
  if (!rootMatch) die('Input is missing root <svg> tag');
  const rootTag = rootMatch[0];
  const viewBoxMatch = rootTag.match(/viewBox\s*=\s*"([^"]+)"/i);

  let vb;
  if (viewBoxMatch) {
    const nums = viewBoxMatch[1].trim().split(/[\s,]+/).map(Number);
    if (nums.length !== 4 || nums.some((n) => !Number.isFinite(n))) {
      die(`Invalid viewBox: ${viewBoxMatch[1]}`);
    }
    vb = { x: nums[0], y: nums[1], w: nums[2], h: nums[3] };
  } else {
    const wMatch = rootTag.match(/width\s*=\s*"([0-9.]+)(px)?"/i);
    const hMatch = rootTag.match(/height\s*=\s*"([0-9.]+)(px)?"/i);
    if (!wMatch || !hMatch) {
      die('Input root <svg> needs either viewBox or width+height');
    }
    vb = { x: 0, y: 0, w: Number(wMatch[1]), h: Number(hMatch[1]) };
  }

  const inner = svgText
    .replace(/^[\s\S]*?<svg\b[^>]*>/i, '')
    .replace(/<\/svg>\s*$/i, '');

  if (!inner.trim()) {
    die('Input svg has no inner content');
  }

  return { viewBox: vb, inner };
}

function fmt(n) {
  return Number(n.toFixed(4)).toString();
}

function makeSlotSvg({ slug, slotIndex, slotViewBox, innerContent }) {
  const clipId = `clip_${slug}_${String(slotIndex + 1).padStart(2, '0')}`;
  const nestedViewBox = `${fmt(slotViewBox.x)} ${fmt(slotViewBox.y)} ${fmt(slotViewBox.w)} ${fmt(slotViewBox.h)}`;

  return [
    `<svg xmlns="http://www.w3.org/2000/svg" width="${OUTER_W}" height="${OUTER_H}" viewBox="0 0 ${OUTER_W} ${OUTER_H}">`,
    `  <defs><clipPath id="${clipId}"><rect x="0" y="0" width="${OUTER_W}" height="${OUTER_H}"/></clipPath></defs>`,
    `  <g clip-path="url(#${clipId})">`,
    `    <svg x="${fmt(INNER_X)}" y="${fmt(INNER_Y)}" width="${fmt(INNER_W)}" height="${fmt(INNER_H)}" viewBox="${nestedViewBox}" preserveAspectRatio="xMidYMid meet" style="overflow:hidden">`,
    innerContent,
    '    </svg>',
    '  </g>',
    '</svg>',
    '',
  ].join('\n');
}

function writeFileEnsured(filePath, content) {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, content, 'utf8');
}

function main() {
  const args = parseArgs(process.argv);
  const root = process.cwd();
  const inputPath = path.resolve(args.input);
  if (!fs.existsSync(inputPath)) {
    die(`Input not found: ${inputPath}`);
  }

  const raw = fs.readFileSync(inputPath, 'utf8');
  const { viewBox, inner } = parseRootSvg(raw);

  const slotW = viewBox.w / args.columns;
  const slotH = viewBox.h / args.rows;

  const outPublicDir = path.join(root, 'screen-record', 'public');
  const outDistDir = path.join(root, 'src', 'overlay', 'screen_record', 'dist');

  CURSOR_TYPES.forEach((type, i) => {
    const col = i % args.columns;
    const row = Math.floor(i / args.columns);
    const slotViewBox = {
      x: viewBox.x + col * slotW,
      y: viewBox.y + row * slotH,
      w: slotW,
      h: slotH,
    };

    const fileName = `cursor-${type}-${args.slug}.svg`;
    const svg = makeSlotSvg({
      slug: args.slug,
      slotIndex: i,
      slotViewBox,
      innerContent: inner,
    });

    writeFileEnsured(path.join(outPublicDir, fileName), svg);
    writeFileEnsured(path.join(outDistDir, fileName), svg);
  });

  console.log(`[cursor-pack] Generated ${CURSOR_TYPES.length} cursor SVGs for '${args.slug}'`);
  console.log(`[cursor-pack] Source: ${inputPath}`);
  console.log(`[cursor-pack] Grid: ${args.columns}x${args.rows}`);
}

main();
