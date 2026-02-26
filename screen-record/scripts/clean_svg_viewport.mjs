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

// Parses the (x, y) of the opening M command of an SVG subpath segment.
// Uses ([-+]?[\d.]+) so the sign is an optional prefix only (not part of
// the repeating class), which lets "221.8-56.8" parse correctly:
//   group1="221.8" ([\d.]+ stops at '-'), group2="-56.8" (sign as prefix).
// Avoids the old [-\d.]+ bug where '-' in the char class greedily consumed
// "221.8-56.8" as one token, causing both M coords to be missed.
const MCOORD_RE = /^[Mm]\s*([-+]?[\d.]+)[,\s]*([-+]?[\d.]+)/;

function mCoords(subpath) {
  const m = subpath.match(MCOORD_RE);
  if (!m) return null;
  return [parseFloat(m[1]), parseFloat(m[2])];
}

function isInsideViewBox([x, y], vb) {
  return x >= vb.x && x <= vb.x + vb.w && y >= vb.y && y <= vb.y + vb.h;
}

/**
 * Filters invisible subpaths from a compound path d attribute.
 *
 * A compound path uses Z...M to start additional subpaths. Each subpath is
 * tested independently: its starting M coordinate is checked against the
 * viewBox. Subpaths with unparseable M commands are kept (conservative).
 *
 * Returns the filtered d string, or null if all subpaths are invisible.
 */
function filterCompoundPath(d, vb) {
  // Split at Z (or z) that is immediately followed by M (or m), preserving Z.
  // "...arcZ M221..." → ["...arcZ", " M221..."]
  const segments = d.split(/(?<=[Zz])(?=\s*[Mm])/);

  const visible = segments.filter(seg => {
    const trimmed = seg.replace(/^[Zz\s]+/, ''); // strip leading Z from continuations
    const coords = mCoords(trimmed);
    if (!coords) return true; // can't determine → keep (conservative)
    return isInsideViewBox(coords, vb);
  });

  if (visible.length === 0) return null;
  return visible.join('');
}

/**
 * Return a cleaned d attribute string for a path, or null to remove the
 * whole path element. Handles both simple and compound paths.
 */
function cleanedPathD(d, vb) {
  const isCompound = /[Zz]\s*[Mm]/.test(d);
  if (isCompound) {
    return filterCompoundPath(d, vb);
  }
  // Simple path: check single M coord.
  const coords = mCoords(d);
  if (!coords) return d; // unparseable → keep
  return isInsideViewBox(coords, vb) ? d : null;
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
