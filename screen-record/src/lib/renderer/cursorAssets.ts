import {
  CURSOR_PACKS,
  CURSOR_RENDER_KINDS,
  getCursorImageFieldName,
  isCursorPack,
  type CursorImageSet,
  type CursorRenderKind,
} from './cursorTypes';
import { initPreviewAnimation } from './cursorAnimationCapture';

// ---------------------------------------------------------------------------
// Cursor asset version cache-buster
// ---------------------------------------------------------------------------
const CURSOR_ASSET_VERSION = `cursor-types-runtime-${Date.now()}`;

export function getCursorAssetUrl(name: string): string {
  return `/${name}.svg?v=${CURSOR_ASSET_VERSION}`;
}

// ---------------------------------------------------------------------------
// createCursorImageSet - loads all cursor pack SVGs into HTMLImageElements
// ---------------------------------------------------------------------------

export function createCursorImageSet(): CursorImageSet {
  const loadImg = (name: string): HTMLImageElement => {
    const img = new Image();
    img.src = getCursorAssetUrl(name);
    return img;
  };

  const images: Record<string, HTMLImageElement> = {};

  for (const pack of CURSOR_PACKS) {
    for (const cursorKind of CURSOR_RENDER_KINDS) {
      const fieldName = getCursorImageFieldName(pack, cursorKind);
      images[fieldName] = loadImg(`cursor-${cursorKind}-${pack}`);
    }
  }

  return images as unknown as CursorImageSet;
}

// ---------------------------------------------------------------------------
// Lazy animation init — only triggered when a project actually needs it
// ---------------------------------------------------------------------------

// Animated cursor types that need preview pre-rendering.
const ANIMATED_CURSOR_TYPES: readonly CursorRenderKind[] = ['wait', 'appstarting'];

// Raw cursor_type values from recordings that map to animated types.
const ANIMATED_RAW_TYPES = new Set(['wait', 'appstarting']);

// Track which packs have already been initialized.
const _initedPacks = new Set<string>();

/**
 * Initialize animated cursor preview/export for the given pack, but ONLY if
 * the project's mouse positions actually contain wait/appstarting moments.
 * Safe to call repeatedly — idempotent per pack.
 */
export function ensureCursorAnimations(
  pack: string,
  mousePositions: { cursor_type?: string }[],
  images: CursorImageSet,
): void {
  if (!isCursorPack(pack) || _initedPacks.has(pack)) return;

  const hasAnimated = mousePositions.some(
    p => ANIMATED_RAW_TYPES.has((p.cursor_type || '').toLowerCase()),
  );
  if (!hasAnimated) return;

  _initedPacks.add(pack);

  for (const type of ANIMATED_CURSOR_TYPES) {
    const fieldName = getCursorImageFieldName(pack, type);
    const img = images[fieldName];
    if (img) initPreviewAnimation(img, `${type}-${pack}`);
  }
}
