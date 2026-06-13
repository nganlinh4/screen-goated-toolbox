#!/usr/bin/env node
// Deterministic i18n gap scanner across all locale dictionary systems in the repo.
// Emits a JSON report: per system, per non-English locale -> missing keys,
// extra keys, placeholder mismatches, and identical-to-English candidates.
import fs from 'node:fs';
import path from 'node:path';

const ROOT = process.argv[2] || process.cwd();

// ---- generic line-based key:"value" / key='value' / key = "value" parser ----
// Returns Map<key, {value, line}> preserving first occurrence.
function parseFlat(content, { sep, quote }) {
  const map = new Map();
  const lines = content.split(/\r?\n/);
  // key separator: ':' or '='
  const sepRe = sep === '=' ? '\\s*=\\s*' : '\\s*:\\s*';
  const q = quote;
  // value: quote, then (non-quote | escaped) *, then quote, then optional comma
  const re = new RegExp(`^\\s*([A-Za-z_][A-Za-z0-9_]*)${sepRe}${q}((?:[^${q}\\\\]|\\\\.)*)${q}\\s*,?\\s*$`);
  for (let i = 0; i < lines.length; i++) {
    const m = lines[i].match(re);
    if (m) {
      const key = m[1];
      if (!map.has(key)) map.set(key, { value: m[2], line: i + 1 });
    }
  }
  return map;
}

// Extract the body of a named sub-object: `en: { ... }` (brace-balanced) from a combined file.
function sliceObject(content, label) {
  const startRe = new RegExp(`(^|\\n)\\s*${label}\\s*:\\s*\\{`);
  const m = content.match(startRe);
  if (!m) return null;
  let i = content.indexOf('{', m.index);
  let depth = 0;
  for (let j = i; j < content.length; j++) {
    if (content[j] === '{') depth++;
    else if (content[j] === '}') { depth--; if (depth === 0) return content.slice(i + 1, j); }
  }
  return null;
}

// placeholders: {clip}, {{x}}, %s, %d, %1$s, $name, {0}
function placeholders(v) {
  const set = new Set();
  for (const re of [/\{\{[^}]+\}\}/g, /\{[^}]*\}/g, /%[0-9]*\$?[sd]/g, /\$[A-Za-z_][A-Za-z0-9_]*/g]) {
    const mm = v.match(re);
    if (mm) mm.forEach((x) => set.add(x));
  }
  return set;
}

function eqSet(a, b) {
  if (a.size !== b.size) return false;
  for (const x of a) if (!b.has(x)) return false;
  return true;
}

function compare(systemName, enMap, locales) {
  const out = { system: systemName, enKeyCount: enMap.size, locales: {} };
  for (const [loc, lmap] of Object.entries(locales)) {
    const missing = [];
    const extra = [];
    const placeholderMismatch = [];
    const identical = [];
    for (const [k, ev] of enMap) {
      if (!lmap.has(k)) { missing.push(k); continue; }
      const lv = lmap.get(k);
      if (!eqSet(placeholders(ev.value), placeholders(lv.value))) {
        placeholderMismatch.push({ key: k, en: ev.value, [loc]: lv.value });
      }
      if (ev.value === lv.value && ev.value.trim() !== '') {
        identical.push({ key: k, value: ev.value, line: lv.line });
      }
    }
    for (const k of lmap.keys()) if (!enMap.has(k)) extra.push(k);
    out.locales[loc] = {
      keyCount: lmap.size,
      missingCount: missing.length,
      extraCount: extra.length,
      placeholderMismatchCount: placeholderMismatch.length,
      identicalCount: identical.length,
      missing,
      extra,
      placeholderMismatch,
      identical,
    };
  }
  return out;
}

const report = { generatedFrom: ROOT, systems: [] };

// 1) screen-record TS i18n (separate files, single-quote)
{
  const dir = path.join(ROOT, 'screen-record/src/i18n');
  const opt = { sep: ':', quote: "'" };
  const en = parseFlat(fs.readFileSync(path.join(dir, 'en.ts'), 'utf8'), opt);
  const ko = parseFlat(fs.readFileSync(path.join(dir, 'ko.ts'), 'utf8'), opt);
  const vi = parseFlat(fs.readFileSync(path.join(dir, 'vi.ts'), 'utf8'), opt);
  report.systems.push(compare('screen-record/src/i18n (TS)', en, { ko, vi }));
}

// 2) promptdj-midi Locales.ts (combined, single-quote)
{
  const file = path.join(ROOT, 'promptdj-midi/utils/Locales.ts');
  const content = fs.readFileSync(file, 'utf8');
  const opt = { sep: ':', quote: "'" };
  const en = parseFlat(sliceObject(content, 'en'), opt);
  const vi = parseFlat(sliceObject(content, 'vi'), opt);
  const ko = parseFlat(sliceObject(content, 'ko'), opt);
  report.systems.push(compare('promptdj-midi/utils/Locales.ts (TS)', en, { ko, vi }));
}

// 3) Rust GUI locale (separate files, double-quote, key: "value")
{
  const dir = path.join(ROOT, 'src/gui/locale');
  const opt = { sep: ':', quote: '"' };
  const en = parseFlat(fs.readFileSync(path.join(dir, 'en.rs'), 'utf8'), opt);
  const ko = parseFlat(fs.readFileSync(path.join(dir, 'ko.rs'), 'utf8'), opt);
  const vi = parseFlat(fs.readFileSync(path.join(dir, 'vi.rs'), 'utf8'), opt);
  report.systems.push(compare('src/gui/locale (Rust)', en, { ko, vi }));
}

// 4) Mobile Kotlin (separate files, double-quote, key = "value")
{
  const dir = path.join(ROOT, 'mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/ui/i18n');
  const opt = { sep: '=', quote: '"' };
  const en = parseFlat(fs.readFileSync(path.join(dir, 'MobileLocaleEn.kt'), 'utf8'), opt);
  const ko = parseFlat(fs.readFileSync(path.join(dir, 'MobileLocaleKo.kt'), 'utf8'), opt);
  const vi = parseFlat(fs.readFileSync(path.join(dir, 'MobileLocaleVi.kt'), 'utf8'), opt);
  report.systems.push(compare('mobile ui/i18n (Kotlin)', en, { ko, vi }));
}

// summary
console.log('=== i18n GAP SUMMARY ===');
for (const s of report.systems) {
  console.log(`\n## ${s.system}  (en keys: ${s.enKeyCount})`);
  for (const [loc, d] of Object.entries(s.locales)) {
    console.log(
      `  ${loc}: keys=${d.keyCount} missing=${d.missingCount} extra=${d.extraCount} ` +
      `placeholderMismatch=${d.placeholderMismatchCount} identicalToEN=${d.identicalCount}`,
    );
  }
}

const outFile = path.join(ROOT, 'scripts/i18n_scan_report.json');
fs.writeFileSync(outFile, JSON.stringify(report, null, 2));
console.log(`\nFull report -> ${outFile}`);
