#!/usr/bin/env node
/**
 * Strips invisible SVG elements that lie outside the viewport.
 *
 * For cursor SVGs generated from spritesheets, an inner <svg> clips to its
 * viewBox via overflow:hidden — but the off-screen paths from other cursor
 * slots are still present in the file. This script removes them, then prunes
 * any <linearGradient> defs that are no longer referenced.
 */

import { readFileSync, writeFileSync, readdirSync } from 'fs';
import { join, basename } from 'path';

const DIRS = [
  'C:/WORK/screen-goated-toolbox/screen-record/public',
  'C:/WORK/screen-goated-toolbox/src/overlay/screen_record/dist',
];

function parseViewBox(vbStr) {
  const [x, y, w, h] = vbStr.trim().split(/[\s,]+/).map(Number);
  return { x, y, w, h };
}

const NUM = '[-+]?[\\d]*\\.?[\\d]+(?:e[-+]?[\\d]+)?';
// Extracts all absolute coordinate pairs from a path d string.
// Tokenises the path into commands + number sequences, tracks current pen
// position to resolve relative commands, and collects every endpoint.
// This is far safer than checking only the M start point: a path that begins
// outside the viewBox but curves/lines into it will still be detected.
function allAbsolutePoints(d) {
  const points = [];
  // Tokenise: split into [command-letter, ...numbers] chunks.
  const tokens = d.match(/[MmZzLlHhVvCcSsQqTtAa]|[-+]?[\d]*\.?[\d]+(?:e[-+]?[\d]+)?/gi) || [];
  let i = 0;
  let cx = 0, cy = 0; // current pen
  let cmd = 'M';
  const nums = () => { const n = []; while (i < tokens.length && /^[-+.\d]/.test(tokens[i])) n.push(parseFloat(tokens[i++])); return n; };
  while (i < tokens.length) {
    const t = tokens[i];
    if (/^[A-Za-z]$/.test(t)) { cmd = t; i++; }
    const rel = cmd === cmd.toLowerCase() && cmd !== 'z' && cmd !== 'Z';
    const abs = (dx, dy) => rel ? [cx + dx, cy + dy] : [dx, dy];
    switch (cmd.toUpperCase()) {
      case 'M': { const n = nums(); if (n.length >= 2) { const [x, y] = abs(n[0], n[1]); points.push([x, y]); cx = x; cy = y; cmd = rel ? 'l' : 'L'; } break; }
      case 'L': { const n = nums(); for (let j = 0; j + 1 < n.length; j += 2) { const [x, y] = abs(n[j], n[j+1]); points.push([x, y]); cx = x; cy = y; } break; }
      case 'H': { const n = nums(); for (const v of n) { cx = rel ? cx + v : v; points.push([cx, cy]); } break; }
      case 'V': { const n = nums(); for (const v of n) { cy = rel ? cy + v : v; points.push([cx, cy]); } break; }
      case 'C': { const n = nums(); for (let j = 0; j + 5 < n.length; j += 6) { const [x, y] = abs(n[j+4], n[j+5]); points.push([x, y]); cx = x; cy = y; } break; }
      case 'S': case 'Q': { const n = nums(); for (let j = 0; j + 3 < n.length; j += 4) { const [x, y] = abs(n[j+2], n[j+3]); points.push([x, y]); cx = x; cy = y; } break; }
      case 'T': { const n = nums(); for (let j = 0; j + 1 < n.length; j += 2) { const [x, y] = abs(n[j], n[j+1]); points.push([x, y]); cx = x; cy = y; } break; }
      case 'A': { const n = nums(); for (let j = 0; j + 6 < n.length; j += 7) { const [x, y] = abs(n[j+5], n[j+6]); points.push([x, y]); cx = x; cy = y; } break; }
      case 'Z': { i++; break; }
      default: i++;
    }
  }
  return points;
}

function isInsideViewBox([x, y], vb) {
  return x >= vb.x && x <= vb.x + vb.w && y >= vb.y && y <= vb.y + vb.h;
}

/** Returns true if any endpoint of the path falls inside the viewBox. */
function pathIntersectsViewBox(d, vb) {
  const pts = allAbsolutePoints(d);
  if (pts.length === 0) return true; // unparseable → keep
  return pts.some(p => isInsideViewBox(p, vb));
}

/**
 * Return a cleaned d attribute string for a path, or null to remove the
 * whole path element.
 * Checks ALL endpoint coordinates (not just M) so paths that start outside
 * but draw into the viewBox are correctly preserved.
 */
function cleanedPathD(d, vb) {
  return pathIntersectsViewBox(d, vb) ? d : null;
}

/** Collect all IDs referenced via url(#id) or href="#id" in an SVG string. */
function collectReferencedIds(svg) {
  const ids = new Set();
  for (const m of svg.matchAll(/url\(#([^)]+)\)/g)) ids.add(m[1]);
  for (const m of svg.matchAll(/(?:xlink:)?href="#([^"]+)"/g)) ids.add(m[1]);
  return ids;
}

/**
 * For the macos26 format: <g transform="..." clip-path="url(#id)"> wraps all paths.
 * The clipPath contains a <rect> whose coordinates are already in the inner <g>'s
 * LOCAL coordinate space — the same space as the M commands in the path d attributes.
 * No transform conversion needed: just read the rect values directly.
 * Returns a viewBox-like object, or null if the pattern is not found.
 */
function detectClipGViewBox(content) {
  const gMatch = content.match(/<g\b[^>]*\bclip-path="url\(#([^)]+)\)"/);
  if (!gMatch) return null;
  const clipId = gMatch[1];

  const clipRe = new RegExp(`<clipPath[^>]*\\bid="${clipId}"[^>]*>[\\s\\S]*?<rect\\b([^/]*)/>`, 'i');
  const clipMatch = content.match(clipRe);
  if (!clipMatch) return null;

  const attrs = clipMatch[1];
  const rx = parseFloat((attrs.match(/\bx="([-\d.]+)"/) || [])[1] ?? '0');
  const ry = parseFloat((attrs.match(/\by="([-\d.]+)"/) || [])[1] ?? '0');
  const rw = parseFloat((attrs.match(/\bwidth="([-\d.]+)"/) || [])[1] ?? '0');
  const rh = parseFloat((attrs.match(/\bheight="([-\d.]+)"/) || [])[1] ?? '0');

  return { x: rx, y: ry, w: rw, h: rh };
}

function cleanSVG(content) {
  // --- Format A: inner <svg viewBox="..." style="overflow:hidden"> ---
  const innerMatch = content.match(/<svg\b([^>]+)\bviewBox="([^"]+)"([^>]*)style="overflow:hidden"/);

  // --- Format B: <g clip-path="url(#...)"> with a <clipPath> rect (macos26 style).
  //     The clipPath rect coordinates are already in the inner <g>'s local space.
  const vb = innerMatch
    ? parseViewBox(innerMatch[2])
    : detectClipGViewBox(content);

  if (!vb) {
    // Nothing to clean — no recognised clipping structure.
    return { content, removed: 0, defsRemoved: 0 };
  }

  let removed = 0;

  // Remove or trim self-closing <path .../> elements outside the viewBox.
  // The `s` flag lets . match newlines so multi-line path attributes work.
  // For compound paths (Z...M), only invisible subpaths are stripped; if at
  // least one subpath is visible, the element is kept with its d rewritten.
  const afterPaths = content.replace(/<path\b[\s\S]*?\/>/gs, (match) => {
    const dMatch = match.match(/\bd="([^"]+)"/);
    if (!dMatch) return match;
    const cleaned = cleanedPathD(dMatch[1], vb);
    if (cleaned === null) {
      // All subpaths invisible — remove the entire element.
      removed++;
      return '';
    }
    if (cleaned !== dMatch[1]) {
      // Some subpaths stripped — rewrite the d attribute.
      removed++;
      return match.replace(/\bd="[^"]+"/, `d="${cleaned}"`);
    }
    return match;
  });

  // Remove <defs>...</defs> blocks that became entirely empty or only contain
  // whitespace after path removal.
  const afterEmptyDefs = afterPaths.replace(/<defs>\s*<\/defs>/g, '');

  // Remove individual <linearGradient> elements whose IDs are no longer
  // referenced anywhere in the (now trimmed) SVG.
  const referencedIds = collectReferencedIds(afterEmptyDefs);
  let defsRemoved = 0;
  const afterOrphanDefs = afterEmptyDefs.replace(
    /<linearGradient\b[^>]*\bid="([^"]+)"[^>]*>[\s\S]*?<\/linearGradient>/g,
    (match, id) => {
      if (!referencedIds.has(id)) {
        defsRemoved++;
        return '';
      }
      return match;
    }
  );

  // One more pass to remove any defs blocks that are now empty.
  const final = afterOrphanDefs.replace(/<defs>\s*<\/defs>/g, '');

  return { content: final, removed, defsRemoved };
}

// ── Main ────────────────────────────────────────────────────────────────────

let totalFiles = 0, totalRemoved = 0, totalDefsRemoved = 0, totalSaved = 0;

for (const dir of DIRS) {
  let files;
  try {
    files = readdirSync(dir).filter(f => f.endsWith('.svg'));
  } catch {
    console.warn(`Skipping missing dir: ${dir}`);
    continue;
  }

  for (const file of files) {
    const filepath = join(dir, file);
    const original = readFileSync(filepath, 'utf8');
    const { content, removed, defsRemoved } = cleanSVG(original);

    if (removed > 0 || defsRemoved > 0) {
      writeFileSync(filepath, content, 'utf8');
      const saved = original.length - content.length;
      totalRemoved += removed;
      totalDefsRemoved += defsRemoved;
      totalSaved += saved;
      totalFiles++;
      console.log(
        `${basename(dir)}/${file}: -${removed} paths, -${defsRemoved} gradients, -${(saved / 1024).toFixed(1)} KB`
      );
    }
  }
}

console.log(
  `\nDone. Cleaned ${totalFiles} files — removed ${totalRemoved} paths, ` +
  `${totalDefsRemoved} orphaned gradients, saved ${(totalSaved / 1024).toFixed(1)} KB total.`
);
