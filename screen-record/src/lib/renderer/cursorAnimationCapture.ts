// Animated cursor frame cache — compute once, save to disk, load instantly.
//
// On first encounter of an animated SVG cursor, freezes SMIL/CSS animations
// at evenly-spaced timestamps via the browser renderer, then saves the
// rendered frames to %LOCALAPPDATA% via Rust IPC. Subsequent loads skip all
// SVG rendering and load straight from disk cache.
//
// Preview uses frozen SVG text as blob URL <img> sources — Chrome renders
// them as vector graphics at the final display resolution, so they stay
// perfectly crisp at any cursor scale. No bitmap resolution limit.

import { invoke } from '@/lib/ipc';
import { pushHeaderStatus } from '@/lib/headerStatus';

// ---------------------------------------------------------------------------
// Duration parsing
// ---------------------------------------------------------------------------

function parseDuration(s: string | null | undefined): number {
  if (!s) return 0;
  s = s.trim();
  if (s.endsWith('ms')) return parseFloat(s) / 1000;
  if (s.endsWith('min')) return parseFloat(s) * 60;
  if (s.endsWith('s')) return parseFloat(s);
  const n = parseFloat(s);
  return isNaN(n) ? 0 : n;
}

// ---------------------------------------------------------------------------
// parseSvgLoopDuration — detect animation duration from SMIL or CSS
// ---------------------------------------------------------------------------

export function parseSvgLoopDuration(svgText: string): number {
  let max = 0;
  for (const m of svgText.matchAll(/\bdur="([^"]+)"/g)) {
    const d = parseDuration(m[1]);
    if (d > max) max = d;
  }
  for (const m of svgText.matchAll(/animation-duration\s*:\s*(\d+\.?\d*)\s*(s|ms)/g)) {
    const val = parseFloat(m[1]);
    const d = m[2] === 'ms' ? val / 1000 : val;
    if (d > max) max = d;
  }
  for (const m of svgText.matchAll(/animation\s*:[^;}]*?(\d+\.?\d*)\s*(s|ms)/g)) {
    const val = parseFloat(m[1]);
    const d = m[2] === 'ms' ? val / 1000 : val;
    if (d > max) max = d;
  }
  return max;
}

// ---------------------------------------------------------------------------
// freezeSvgAtTime — freeze both SMIL and CSS animations at wall-clock t
// ---------------------------------------------------------------------------

function freezeSvgAtTime(svgText: string, t: number): string {
  const parser = new DOMParser();
  const doc = parser.parseFromString(svgText, 'image/svg+xml');
  for (const tag of ['animate', 'animateTransform', 'animateMotion', 'set']) {
    for (const el of doc.querySelectorAll(tag)) {
      el.setAttribute('begin', `-${t}s`);
    }
  }
  if (/animation\s*:/i.test(svgText)) {
    const styleEl = doc.createElementNS('http://www.w3.org/2000/svg', 'style');
    styleEl.textContent =
      `* { animation-delay: -${t}s !important; animation-play-state: paused !important; }`;
    doc.documentElement.insertBefore(styleEl, doc.documentElement.firstChild);
  }
  return new XMLSerializer().serializeToString(doc.documentElement);
}

// ---------------------------------------------------------------------------
// Simple string hash for cache invalidation
// ---------------------------------------------------------------------------

function hashString(s: string): string {
  let h = 0;
  for (let i = 0; i < s.length; i++) {
    h = ((h << 5) - h + s.charCodeAt(i)) | 0;
  }
  return (h >>> 0).toString(36);
}

// ---------------------------------------------------------------------------
// Atlas slot IDs — must match cursor_type_to_id in Rust cursors.rs
// ---------------------------------------------------------------------------

const CURSOR_TYPE_TO_SLOT: Readonly<Record<string, number>> = {
  'wait-screenstudio': 5,
  'appstarting-screenstudio': 6,
  'wait-macos26': 17,
  'appstarting-macos26': 18,
  'wait-sgtcute': 29,
  'appstarting-sgtcute': 30,
  'wait-sgtcool': 41,
  'appstarting-sgtcool': 42,
  'wait-sgtai': 53,
  'appstarting-sgtai': 54,
  'wait-sgtpixel': 65,
  'appstarting-sgtpixel': 66,
  'wait-jepriwin11': 77,
  'appstarting-jepriwin11': 78,
  'wait-sgtwatermelon': 89,
  'appstarting-sgtwatermelon': 90,
  'wait-sgtfastfood': 101,
  'appstarting-sgtfastfood': 102,
};
export function getCursorAtlasSlotId(cursorType: string): number {
  return CURSOR_TYPE_TO_SLOT[cursorType] ?? -1;
}

// ---------------------------------------------------------------------------
// Preview store — frozen SVG blob URL images (vector quality at any scale)
// ---------------------------------------------------------------------------

const CAPTURE_FPS = 60;
const EXPORT_TILE = 512;

interface PreviewAnimData {
  // Blob URL <img> elements pointing to frozen SVGs — Chrome renders these
  // as vector graphics at the final display size. No bitmap resolution limit.
  images: HTMLImageElement[];
  loopDuration: number;
}

const _previewStore = new Map<HTMLImageElement, PreviewAnimData | 'loading' | null>();

// ---------------------------------------------------------------------------
// Status tracking — show header badge while computing animation frames
// ---------------------------------------------------------------------------

let _pendingCount = 0;

function markPending(): void {
  _pendingCount++;
  if (_pendingCount === 1) {
    pushHeaderStatus('cursor-anim', 'statusPreparingCursors');
  }
}

function markDone(): void {
  _pendingCount = Math.max(0, _pendingCount - 1);
  if (_pendingCount === 0) {
    pushHeaderStatus('cursor-anim', 'statusCursorsReady', 'success', 2000);
  }
}

// ---------------------------------------------------------------------------
// initPreviewAnimation — main entry point, called per animated cursor image
// ---------------------------------------------------------------------------

export function initPreviewAnimation(img: HTMLImageElement, cursorType: string): void {
  if (_previewStore.has(img)) return;
  _previewStore.set(img, 'loading');
  markPending();

  const slotId = getCursorAtlasSlotId(cursorType);

  (async () => {
    try {
      const srcUrl = img.src;
      if (!srcUrl) { _previewStore.set(img, null); markDone(); return; }

      const resp = await fetch(srcUrl);
      if (!resp.ok) { _previewStore.set(img, null); markDone(); return; }
      const svgText = await resp.text();

      const loopDuration = parseSvgLoopDuration(svgText);
      if (loopDuration <= 0) { _previewStore.set(img, null); markDone(); return; }

      const svgHash = hashString(svgText);

      // Try loading from disk cache first (instant).
      if (slotId >= 0) {
        const cached = await tryLoadFromCache(slotId, svgHash, img);
        if (cached) { markDone(); return; }
      }

      // Cache miss — compute frames from SVG (one-time only).
      await computeAndCache(svgText, loopDuration, svgHash, slotId, img);
      markDone();
    } catch (e) {
      console.warn('[CursorAnim] init failed:', e);
      _previewStore.set(img, null);
      markDone();
    }
  })();
}

// ---------------------------------------------------------------------------
// Cache hit path — load frozen SVG strings from disk, create blob URL images
// ---------------------------------------------------------------------------

async function tryLoadFromCache(
  slotId: number,
  svgHash: string,
  img: HTMLImageElement,
): Promise<boolean> {
  try {
    const result = await invoke('load_cursor_anim_cache', { slotId, svgHash }) as {
      cached: boolean;
      loopDuration?: number;
      naturalWidth?: number;
      naturalHeight?: number;
      previewFrames?: string[];
    };
    if (!result.cached || !result.previewFrames || result.previewFrames.length === 0) return false;

    // Reconstruct vector-quality preview images from cached frozen SVG strings.
    const images = await svgStringsToImages(result.previewFrames);

    _previewStore.set(img, {
      images,
      loopDuration: result.loopDuration!,
    });
    return true;
  } catch {
    return false;
  }
}

// Create blob URL HTMLImageElements from base64-encoded frozen SVG text.
// Chrome renders these as vector graphics — crisp at any cursor scale.
async function svgStringsToImages(b64Strings: string[]): Promise<HTMLImageElement[]> {
  const images: HTMLImageElement[] = [];
  for (const b64 of b64Strings) {
    const svgBytes = Uint8Array.from(atob(b64), c => c.charCodeAt(0));
    const blob = new Blob([svgBytes], { type: 'image/svg+xml' });
    const url = URL.createObjectURL(blob);
    const frameImg = new Image();
    await new Promise<void>((resolve) => {
      frameImg.onload = () => resolve();
      frameImg.onerror = () => resolve();
      frameImg.src = url;
    });
    // Don't revoke — blob URL must stay alive for preview rendering.
    images.push(frameImg);
  }
  return images;
}

// ---------------------------------------------------------------------------
// Cache miss path — capture frames from SVG, save to disk, populate stores
// ---------------------------------------------------------------------------

async function computeAndCache(
  svgText: string,
  loopDuration: number,
  svgHash: string,
  slotId: number,
  img: HTMLImageElement,
): Promise<void> {
  const frameCount = Math.max(2, Math.ceil(CAPTURE_FPS * loopDuration));

  const exportCanvas = document.createElement('canvas');
  exportCanvas.width = EXPORT_TILE;
  exportCanvas.height = EXPORT_TILE;
  const exportCtx = exportCanvas.getContext('2d')!;

  let exDrawX = 0, exDrawY = 0, exDrawW = 0, exDrawH = 0;
  let transformReady = false;
  let naturalWidth = 44, naturalHeight = 43;

  const previewImages: HTMLImageElement[] = [];
  const previewSvgB64: string[] = [];
  const exportPngsB64: string[] = [];

  for (let i = 0; i < frameCount; i++) {
    const t = (i / frameCount) * loopDuration;
    const frozenSvg = freezeSvgAtTime(svgText, t);

    // Store frozen SVG text for cache (base64-encoded for IPC).
    previewSvgB64.push(btoa(frozenSvg));

    // Create blob URL Image for vector-quality preview.
    const blob = new Blob([frozenSvg], { type: 'image/svg+xml' });
    const url = URL.createObjectURL(blob);

    const frameImg = new Image();
    await new Promise<void>((resolve) => {
      frameImg.onload = () => {
        if (i === 0) {
          naturalWidth = frameImg.naturalWidth > 0 ? frameImg.naturalWidth : 44;
          naturalHeight = frameImg.naturalHeight > 0 ? frameImg.naturalHeight : 43;
        }

        // Render export frame (512×512, Rust atlas centering math).
        if (!transformReady) {
          const target = EXPORT_TILE * 0.94;
          const scale = target / Math.max(naturalWidth, naturalHeight);
          exDrawW = naturalWidth * scale;
          exDrawH = naturalHeight * scale;
          exDrawX = EXPORT_TILE / 2 - (naturalWidth * 0.5 * scale);
          exDrawY = EXPORT_TILE / 2 - (naturalHeight * 0.5 * scale);
          transformReady = true;
        }
        exportCtx.clearRect(0, 0, EXPORT_TILE, EXPORT_TILE);
        exportCtx.drawImage(frameImg, exDrawX, exDrawY, exDrawW, exDrawH);
        exportPngsB64.push(
          exportCanvas.toDataURL('image/png').replace(/^data:image\/png;base64,/, ''),
        );
        resolve();
      };
      frameImg.onerror = () => {
        URL.revokeObjectURL(url);
        exportPngsB64.push('');
        resolve();
      };
      frameImg.src = url;
    });
    // Don't revoke blob URL — keep alive for preview rendering.
    previewImages.push(frameImg);
  }

  // Populate preview store immediately.
  _previewStore.set(img, { images: previewImages, loopDuration });

  // Save to disk cache + populate Rust export store (background).
  if (slotId >= 0) {
    try {
      await invoke('save_cursor_anim_cache', {
        slotId,
        svgHash,
        loopDuration,
        naturalWidth,
        naturalHeight,
        exportPngs: exportPngsB64,
        previewFrames: previewSvgB64,
      });
    } catch (e) {
      console.warn('[CursorAnim] save cache failed:', e);
    }
  }
}

// ---------------------------------------------------------------------------
// getPreviewFrame — called every render frame by drawCenteredCursorImage
// ---------------------------------------------------------------------------

// Returns the current animation frame as an HTMLImageElement with a blob URL
// pointing to a frozen SVG. Chrome renders it as vector graphics at the
// final display resolution — perfectly crisp at any cursor scale.
export function getPreviewFrame(img: HTMLImageElement): HTMLImageElement | null {
  const entry = _previewStore.get(img);
  if (!entry || entry === 'loading' || entry === null) return null;
  if (entry.images.length === 0 || entry.loopDuration <= 0) return null;

  const t = (performance.now() / 1000) % entry.loopDuration;
  const n = entry.images.length;
  const idx = Math.floor((t / entry.loopDuration) * n) % n;
  const frame = entry.images[idx];
  // Only return frames that actually loaded — broken blob URLs fall through to static cursor.
  if (!frame.complete || frame.naturalWidth === 0) return null;
  return frame;
}
