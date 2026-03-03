#!/usr/bin/env node
/**
 * Wires a new cursor pack into all required app files automatically.
 *
 * Usage (run from repo root):
 *   node screen-record/scripts/add_cursor_pack.mjs --slug sgtnew --name "SGT New"
 *   node screen-record/scripts/add_cursor_pack.mjs --slug sgtnew --name "SGT New" --spritesheet ~/Desktop/sheet.svg
 *
 * What it does:
 *   1. Detects the previous pack and next slot IDs from native_export/cursor.rs
 *   2. Optionally generates per-cursor SVGs from a spritesheet
 *   3. Strips off-screen paths via clean_svg_viewport.mjs
 *   4. Patches all 10 TypeScript + Rust source files
 *
 * After running, verify with:
 *   cd screen-record && npx tsc --noEmit
 *   cargo clippy --all-targets
 */

import fs from 'node:fs';
import path from 'node:path';
import { execSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, '..', '..');
const SCRIPTS = path.join(ROOT, 'screen-record', 'scripts');

const CURSOR_TYPES = [
  'default', 'text', 'pointer', 'openhand', 'closehand',
  'wait', 'appstarting', 'crosshair',
  'resize-ns', 'resize-we', 'resize-nwse', 'resize-nesw',
];

// CursorImageSet camelCase field prefixes per cursor type
const FIELD_PREFIX = {
  'default':    'default',
  'text':       'text',
  'pointer':    'pointer',
  'openhand':   'openHand',
  'closehand':  'closeHand',
  'wait':       'wait',
  'appstarting':'appStarting',
  'crosshair':  'crosshair',
  'resize-ns':  'resizeNs',
  'resize-we':  'resizeWe',
  'resize-nwse':'resizeNwse',
  'resize-nesw':'resizeNesw',
};

// Rust const name prefixes per cursor type
const RUST_PREFIX = {
  'default':    'DEFAULT',
  'text':       'TEXT',
  'pointer':    'POINTER',
  'openhand':   'OPENHAND',
  'closehand':  'CLOSEHAND',
  'wait':       'WAIT',
  'appstarting':'APPSTARTING',
  'crosshair':  'CROSSHAIR',
  'resize-ns':  'RESIZE_NS',
  'resize-we':  'RESIZE_WE',
  'resize-nwse':'RESIZE_NWSE',
  'resize-nesw':'RESIZE_NESW',
};

// ── Helpers ──────────────────────────────────────────────────────────────────

function die(msg) { console.error(`[add-cursor-pack] ERROR: ${msg}`); process.exit(1); }
function warn(msg) { console.warn(`[add-cursor-pack] WARN: ${msg}`); }

function parseArgs(argv) {
  const args = {};
  for (let i = 2; i < argv.length; i++) {
    if      (argv[i] === '--slug')        args.slug = argv[++i];
    else if (argv[i] === '--name')        args.name = argv[++i];
    else if (argv[i] === '--spritesheet') args.spritesheet = argv[++i];
    else if (argv[i] === '--help' || argv[i] === '-h') {
      console.log('Usage: node screen-record/scripts/add_cursor_pack.mjs --slug <slug> --name "Display Name" [--spritesheet path.svg]');
      process.exit(0);
    } else die(`Unknown arg: ${argv[i]}`);
  }
  if (!args.slug) die('Missing --slug');
  if (!args.name) die('Missing --name');
  if (!/^[a-z][a-z0-9]*$/.test(args.slug)) die('--slug must be lowercase alphanumeric (no hyphens/underscores)');
  return args;
}

function cap(s) { return s.charAt(0).toUpperCase() + s.slice(1); }
function escRe(s) { return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'); }

function patchFile(filePath, patcher) {
  const original = fs.readFileSync(filePath, 'utf8');
  const patched = patcher(original);
  if (patched === original) {
    warn(`No changes in ${path.relative(ROOT, filePath)} — already patched or pattern mismatch`);
  } else {
    fs.writeFileSync(filePath, patched, 'utf8');
    console.log(`  [OK] ${path.relative(ROOT, filePath)}`);
  }
}

// ── Detect previous pack from cursor.rs ─────────────────────────────────────

function detectPrev(cursorRsPath) {
  const content = fs.readFileSync(cursorRsPath, 'utf8');
  const matches = [...content.matchAll(/"default-([^"]+)" => (\d+)\.0,/g)];
  if (!matches.length) die('Could not detect existing packs from native_export/cursor.rs');
  matches.sort((a, b) => parseInt(a[2]) - parseInt(b[2]));
  const last = matches[matches.length - 1];
  const prevSlug = last[1];
  const prevBase = parseInt(last[2]);
  return {
    prevSlug,
    baseSlot: prevBase + 12,
    seenSize: prevBase + 12 + 12,
  };
}

// ── Per-file patchers ────────────────────────────────────────────────────────

function patchVideoTs(content, slug) {
  return content.replace(
    /(cursor(?:Pack|DefaultVariant|TextVariant|PointerVariant|OpenHandVariant)\?[^;]+);/g,
    (match, body) => body.includes(`'${slug}'`) ? match : `${body} | '${slug}';`,
  );
}

function patchCursorPanel(content, slug, displayName, prevSlug) {
  const newField  = `${slug}Src`;
  const prevField = `${prevSlug}Src`;
  const suffix    = cap(slug);

  // 1. CursorVariant union type
  content = content.replace(
    /('(?:screenstudio|macos26|sgtcute|sgtcool|sgtai|sgtpixel|[a-z0-9]+)'(?:\s*\|\s*'[a-z0-9]+')*);/,
    (m, body) => body.includes(`'${slug}'`) ? m : `${body} | '${slug}';`,
  );

  // 2. CursorVariantRow interface: add field after prevSlug field
  content = content.replace(
    new RegExp(`(  ${escRe(prevField)}: string;\\n)`),
    `$1  ${newField}: string;\n`,
  );

  // 3. Row data: insert new src field before closing } of each row
  //    Anchors on "prevSlugSrc: '/cursor-TYPE-prevSlug.svg' }"
  content = content.replace(
    new RegExp(`${escRe(prevField)}: '/cursor-([^']+)-${escRe(prevSlug)}\\.svg' }`, 'g'),
    `${prevField}: '/cursor-$1-${prevSlug}.svg', ${newField}: '/cursor-$1-${slug}.svg' }`,
  );

  // 4. grid-cols: increment the value used by the cursor variant grid
  //    Find the value that appears on the column header div line
  const colMatch = content.match(/cursor-variant-column-header[^>]+grid-cols-(\d+)/);
  if (colMatch) {
    const colN = parseInt(colMatch[1]);
    content = content.replace(new RegExp(`\\bgrid-cols-${colN}\\b`, 'g'), `grid-cols-${colN + 1}`);
  } else {
    warn('grid-cols pattern not found in CursorPanel.tsx');
  }

  // 5. Column header names array: add displayName before ].map(
  content = content.replace(
    /(\[(?:'[^']+',\s*)*'[^']+'\])(\.map\(\(name\))/,
    (m, arr, tail) => {
      if (arr.includes(`'${displayName}'`)) return m;
      return `${arr.slice(0, -1)}, '${displayName}']${tail}`;
    },
  );

  // 6. variantKeys array: add entry after prevSlug entry
  content = content.replace(
    new RegExp(`(\\{ pack: '${escRe(prevSlug)}', src: row\\.${escRe(prevField)} \\},?)`),
    `$1\n                    { pack: '${slug}', src: row.${newField} },`,
  );

  return content;
}

function patchCursorTypes(content, slug, prevSlug, baseSlot) {
  const suffix     = cap(slug);
  const prevSuffix = cap(prevSlug);

  // 1. CursorRenderType union: extend after last prevSlug entry
  const newRenderEntries = CURSOR_TYPES.map(t => `  | '${t}-${slug}'`).join('\n');
  content = content.replace(
    new RegExp(`(  \\| 'resize-nesw-${escRe(prevSlug)}');`),
    `$1\n${newRenderEntries};`,
  );

  // 2. CursorImageSet interface: add 12 fields before closing }
  const newImageFields = [
    `\n  // ${suffix} pack`,
    ...CURSOR_TYPES.map(t => `  ${FIELD_PREFIX[t]}${suffix}Image: HTMLImageElement;`),
  ].join('\n');
  content = content.replace(
    new RegExp(`(  resizeNesw${prevSuffix}Image: HTMLImageElement;\\n})`),
    `  resizeNesw${prevSuffix}Image: HTMLImageElement;\n${newImageFields}\n}`,
  );

  // 3. getCursorPack return type: add | 'slug' to return type annotation
  content = content.replace(
    new RegExp(`(getCursorPack[^:]+:\\s*'screenstudio'(?:\\s*\\|\\s*'[^']+')*)`),
    (m) => m.includes(`'${slug}'`) ? m : `${m} | '${slug}'`,
  );

  // 4. getCursorPack body: add check before prevSlug check
  content = content.replace(
    new RegExp(`(  if \\(backgroundConfig\\?\\.cursorPack === '${escRe(prevSlug)}'\\) return '${escRe(prevSlug)}';)`),
    `if (backgroundConfig?.cursorPack === '${slug}') return '${slug}';\n  $1`,
  );

  // 5. getCursorPack variant checks: add sgtwatermelon block before prevSlug variant block
  const prevVariantBlock = [
    `  if (backgroundConfig?.cursorDefaultVariant === '${prevSlug}'`,
    `    || backgroundConfig?.cursorTextVariant === '${prevSlug}'`,
    `    || backgroundConfig?.cursorPointerVariant === '${prevSlug}'`,
    `    || backgroundConfig?.cursorOpenHandVariant === '${prevSlug}') {`,
    `    return '${prevSlug}';`,
    `  }`,
  ].join('\n');
  const newVariantBlock = [
    `  if (backgroundConfig?.cursorDefaultVariant === '${slug}'`,
    `    || backgroundConfig?.cursorTextVariant === '${slug}'`,
    `    || backgroundConfig?.cursorPointerVariant === '${slug}'`,
    `    || backgroundConfig?.cursorOpenHandVariant === '${slug}') {`,
    `    return '${slug}';`,
    `  }`,
  ].join('\n');
  content = content.replace(prevVariantBlock, `${newVariantBlock}\n${prevVariantBlock}`);

  // 6. resolveCursorRenderType: add switch block before prevSlug switch block
  const prevSwitchBlock =
    `  if (pack === '${prevSlug}') {\n    switch (semanticType) {`;
  const newSwitchBlock = [
    `  if (pack === '${slug}') {`,
    `    switch (semanticType) {`,
    `      case 'text': return 'text-${slug}';`,
    `      case 'pointer': return 'pointer-${slug}';`,
    `      case 'openhand': return 'openhand-${slug}';`,
    `      case 'closehand': return 'closehand-${slug}';`,
    `      case 'wait': return 'wait-${slug}';`,
    `      case 'appstarting': return 'appstarting-${slug}';`,
    `      case 'crosshair': return 'crosshair-${slug}';`,
    `      case 'resize_ns': return 'resize-ns-${slug}';`,
    `      case 'resize_we': return 'resize-we-${slug}';`,
    `      case 'resize_nwse': return 'resize-nwse-${slug}';`,
    `      case 'resize_nesw': return 'resize-nesw-${slug}';`,
    `      default: return 'default-${slug}';`,
    `    }`,
    `  }`,
    ``,
  ].join('\n');
  content = content.replace(prevSwitchBlock, `${newSwitchBlock}  if (pack === '${prevSlug}') {\n    switch (semanticType) {`);

  // 7. Add getter function before getScreenStudioCursorImage
  const newGetter = [
    `export function get${suffix}CursorImage(images: CursorImageSet, type: CursorRenderType): HTMLImageElement | null {`,
    `  switch (type) {`,
    ...CURSOR_TYPES.map(t => `    case '${t}-${slug}': return images.${FIELD_PREFIX[t]}${suffix}Image;`),
    `    default: return null;`,
    `  }`,
    `}`,
    ``,
  ].join('\n');
  content = content.replace(
    /export function getScreenStudioCursorImage/,
    `${newGetter}export function getScreenStudioCursorImage`,
  );

  return content;
}

function patchCursorAssets(content, slug) {
  const suffix = cap(slug);
  // packs array
  content = content.replace(
    /(\] as const;)/,
    `  { slug: '${slug}', suffix: '${suffix}' },\n$1`,
  );
  // PACK_SUFFIXES
  content = content.replace(
    /(const PACK_SUFFIXES[^}]+})/s,
    (m) => m.includes(`${slug}:`) ? m : m.replace(/};/, `  ${slug}: '${suffix}',\n};`),
  );
  return content;
}

function patchCursorAnimationCapture(content, slug, baseSlot) {
  const waitSlot = baseSlot + 5;        // wait is offset 5
  const appStartingSlot = baseSlot + 6; // appstarting is offset 6
  content = content.replace(
    /(\};)\s*\nexport function getCursorAtlasSlotId/,
    `  'wait-${slug}': ${waitSlot},\n  'appstarting-${slug}': ${appStartingSlot},\n$1\nexport function getCursorAtlasSlotId`,
  );
  return content;
}

function patchCursorGraphics(content, slug, prevSlug) {
  const suffix     = cap(slug);
  const prevSuffix = cap(prevSlug);
  const fnName     = `get${suffix}CursorImage`;
  const prevFnName = `get${prevSuffix}CursorImage`;

  // 1. import: add after prevSlug import
  content = content.replace(
    new RegExp(`(  ${escRe(prevFnName)},\\n}) from './cursorTypes';`),
    `$1  ${fnName},\n} from './cursorTypes';`,
  );

  // 2. re-export: add after prevSlug re-export
  content = content.replace(
    new RegExp(`(  ${escRe(prevFnName)},\\n  getScreenStudioCursorImage,\\n}) from './cursorTypes';`),
    `  ${prevFnName},\n  ${fnName},\n  getScreenStudioCursorImage,\n} from './cursorTypes';`,
  );

  // 3. type guard: add after prevSlug guard block
  const prevGuard = [
    `  if (effectiveType.endsWith('-${prevSlug}')) {`,
    `    const image = ${prevFnName}(images, effectiveType as CursorRenderType);`,
    `    if (!image || !image.complete || image.naturalWidth === 0) {`,
    `      effectiveType = 'default-screenstudio';`,
    `    }`,
    `  }`,
  ].join('\n');
  const newGuard = [
    `  if (effectiveType.endsWith('-${slug}')) {`,
    `    const image = ${fnName}(images, effectiveType as CursorRenderType);`,
    `    if (!image || !image.complete || image.naturalWidth === 0) {`,
    `      effectiveType = 'default-screenstudio';`,
    `    }`,
    `  }`,
  ].join('\n');
  content = content.replace(prevGuard, `${prevGuard}\n${newGuard}`);

  // 4. debug logger chain: add before getScreenStudioCursorImage call
  content = content.replace(
    new RegExp(`(      ${escRe(prevFnName)}\\(images, effectiveType as CursorRenderType\\);)`),
    `$1 ??\n      ${fnName}(images, effectiveType as CursorRenderType);`,
  );

  // 5. switch cases: add block before 'default-screenstudio' case
  const newCases = [
    `    case 'default-${slug}':`,
    ...CURSOR_TYPES.filter(t => t !== 'default').map(t => `    case '${t}-${slug}':`),
    `    case 'resize-nesw-${slug}': {`,
    `      const img = ${fnName}(images, effectiveType);`,
    `      if (img) drawCenteredCursorImage(ctx, img);`,
    `      break;`,
    `    }`,
    ``,
  ].join('\n');
  content = content.replace(
    /    case 'default-screenstudio': \{/,
    `${newCases}    case 'default-screenstudio': {`,
  );

  return content;
}

function patchCursorDynamics(content, slug, prevSlug) {
  // pointer/openhand/closehand hotspot
  content = content.replace(
    new RegExp(`(    case 'closehand-${escRe(prevSlug)}':)`),
    [
      `$1`,
      `    case 'pointer-${slug}':`,
      `    case 'openhand-${slug}':`,
      `    case 'closehand-${slug}':`,
    ].join('\n'),
  );
  // text hotspot
  content = content.replace(
    new RegExp(`(    case 'text-${escRe(prevSlug)}':)`),
    `$1\n    case 'text-${slug}':`,
  );
  return content;
}

function patchCursorSvgLab(content, slug, displayName, prevSlug) {
  const suffix = cap(slug);
  const prevConst = `JEPRIWIN11_ITEMS`; // Always anchors on last-known const; detect dynamically:
  // Find the last XXXX_ITEMS const before CURSOR_ITEMS to use as anchor
  const constsMatch = [...content.matchAll(/^const ([A-Z0-9_]+_ITEMS): CursorItem\[\]/gm)];
  const lastConst = constsMatch[constsMatch.length - 1]?.[1];
  if (!lastConst) { warn('Could not detect last ITEMS const in CursorSvgLab.tsx'); return content; }

  const newConst = `${slug.toUpperCase()}_ITEMS`;
  const newBlock = [
    `const ${newConst}: CursorItem[] = CURSOR_TYPES.map((t) => ({`,
    `  key: \`${slug}-\${t.id}\`,`,
    `  label: \`${displayName} • \${t.label}\`,`,
    `  src: \`/cursor-\${t.id}-${slug}.svg\`,`,
    `}));`,
  ].join('\n');

  // Add const after last ITEMS const
  content = content.replace(
    new RegExp(`(const ${lastConst}[\\s\\S]+?\\}\\)\\);)`),
    `$1\n${newBlock}`,
  );
  // Add spread into CURSOR_ITEMS
  content = content.replace(
    new RegExp(`(  \\.\\.\\.${lastConst},\\n\\];)`),
    `  ...${lastConst},\n  ...${newConst},\n];`,
  );
  return content;
}

function patchCursorsRs(content, slug) {
  const UPPER = slug.toUpperCase();
  const suffix = cap(slug);

  // 1. Add include_bytes! constants after last jepriwin11/prevSlug block
  const newConsts = CURSOR_TYPES.map(t => {
    const p = RUST_PREFIX[t];
    const constName = `${p}_${UPPER}_SVG`;
    // Use multi-line form for long names
    const line = `const ${constName}: &[u8] = include_bytes!("../dist/cursor-${t}-${slug}.svg");`;
    return line.length > 100
      ? `const ${constName}: &[u8] =\n    include_bytes!("../dist/cursor-${t}-${slug}.svg");`
      : line;
  }).join('\n');

  content = content.replace(
    /(const RESIZE_NESW_[A-Z0-9]+_SVG[^;]+;)\n\n(pub\(super\))/,
    `$1\n${newConsts}\n\n$2`,
  );

  // 2. Add entries to CURSOR_SVG_DATA array after prevSlug block
  const prevUpper = content.match(/\/\/ (\w+)\n    DEFAULT_\w+_SVG,\n[\s\S]+?RESIZE_NESW_\w+_SVG,\n\];/)?.[1];
  const newEntries = [
    `    // ${slug}`,
    ...CURSOR_TYPES.map(t => `    ${RUST_PREFIX[t]}_${UPPER}_SVG,`),
  ].join('\n');
  content = content.replace(
    /(\n\];)$/m,
    `\n${newEntries}\n];`,
  );

  return content;
}

function patchNativeExportCursorRs(content, slug, baseSlot, seenSize) {
  // 1. Add cursor type ID entries before "other"
  const newEntries = CURSOR_TYPES.map((t, i) =>
    `        "${t}-${slug}" => ${(baseSlot + i).toFixed(1)},`,
  ).join('\n');
  content = content.replace(
    /        "other" => 12\.0,/,
    `${newEntries}\n        "other" => 12.0,`,
  );
  // 2. Update seen array size
  content = content.replace(
    /let mut seen = \[false; \d+\];/,
    `let mut seen = [false; ${seenSize}];`,
  );
  return content;
}

// ── Main ─────────────────────────────────────────────────────────────────────

async function main() {
  const { slug, name: displayName, spritesheet } = parseArgs(process.argv);
  const suffix = cap(slug);

  const cursorRsPath = path.join(ROOT, 'src/overlay/screen_record/native_export/cursor.rs');
  const { prevSlug, baseSlot, seenSize } = detectPrev(cursorRsPath);

  console.log(`\n[add-cursor-pack] Adding pack: '${slug}' (${displayName})`);
  console.log(`  Previous pack:  ${prevSlug}`);
  console.log(`  Base slot:      ${baseSlot}–${baseSlot + 11}`);
  console.log(`  seen[] size:    ${seenSize}`);
  console.log();

  // Step 1: Generate SVG files if spritesheet provided
  if (spritesheet) {
    console.log('[1/3] Generating cursor SVGs from spritesheet...');
    execSync(
      `node "${path.join(SCRIPTS, 'generate_cursor_pack.mjs')}" --input "${spritesheet}" --slug "${slug}"`,
      { stdio: 'inherit', cwd: ROOT },
    );
  } else {
    // Check that SVG files exist
    const pubDir = path.join(ROOT, 'screen-record/public');
    const missing = CURSOR_TYPES.filter(t => !fs.existsSync(path.join(pubDir, `cursor-${t}-${slug}.svg`)));
    if (missing.length) {
      die(`SVG files missing for types: ${missing.join(', ')}\nProvide --spritesheet or generate them first with generate_cursor_pack.mjs`);
    }
    console.log('[1/3] SVG files already present, skipping generation.');
  }

  // Step 2: Clean off-screen paths
  console.log('[2/3] Cleaning off-screen paths...');
  execSync(`node "${path.join(SCRIPTS, 'clean_svg_viewport.mjs')}"`, { stdio: 'inherit', cwd: ROOT });

  // Step 3: Patch source files
  console.log('[3/3] Patching source files...');

  const files = {
    videoTs:               path.join(ROOT, 'screen-record/src/types/video.ts'),
    cursorPanel:           path.join(ROOT, 'screen-record/src/components/sidepanel/CursorPanel.tsx'),
    cursorTypes:           path.join(ROOT, 'screen-record/src/lib/renderer/cursorTypes.ts'),
    cursorAssets:          path.join(ROOT, 'screen-record/src/lib/renderer/cursorAssets.ts'),
    cursorAnimCapture:     path.join(ROOT, 'screen-record/src/lib/renderer/cursorAnimationCapture.ts'),
    cursorGraphics:        path.join(ROOT, 'screen-record/src/lib/renderer/cursorGraphics.ts'),
    cursorDynamics:        path.join(ROOT, 'screen-record/src/lib/renderer/cursorDynamics.ts'),
    cursorSvgLab:          path.join(ROOT, 'screen-record/src/components/CursorSvgLab.tsx'),
    cursorsRs:             path.join(ROOT, 'src/overlay/screen_record/gpu_export/cursors.rs'),
    nativeExportCursorRs:  cursorRsPath,
  };

  patchFile(files.videoTs,            c => patchVideoTs(c, slug));
  patchFile(files.cursorPanel,        c => patchCursorPanel(c, slug, displayName, prevSlug));
  patchFile(files.cursorTypes,        c => patchCursorTypes(c, slug, prevSlug, baseSlot));
  patchFile(files.cursorAssets,       c => patchCursorAssets(c, slug));
  patchFile(files.cursorAnimCapture,  c => patchCursorAnimationCapture(c, slug, baseSlot));
  patchFile(files.cursorGraphics,     c => patchCursorGraphics(c, slug, prevSlug));
  patchFile(files.cursorDynamics,     c => patchCursorDynamics(c, slug, prevSlug));
  patchFile(files.cursorSvgLab,       c => patchCursorSvgLab(c, slug, displayName, prevSlug));
  patchFile(files.cursorsRs,          c => patchCursorsRs(c, slug));
  patchFile(files.nativeExportCursorRs, c => patchNativeExportCursorRs(c, slug, baseSlot, seenSize));

  console.log(`
[add-cursor-pack] Done! Next steps:
  1. cd screen-record && npx tsc --noEmit    (check TypeScript)
  2. cargo clippy --all-targets              (check Rust)
  3. Open Cursor Lab to fine-tune offsets:   http://localhost:5173/#cursor-lab
  4. Run sprite preview:                     node screen-record/scripts/export_cursor_sprite.mjs --slug ${slug}
`);
}

main().catch(e => { console.error(e); process.exit(1); });
