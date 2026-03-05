import { CursorImageSet } from './cursorGraphics';
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

  // Cursor types in consistent order per pack
  const types = [
    'default', 'text', 'pointer', 'openhand', 'closehand',
    'wait', 'appstarting', 'crosshair', 'resize-ns', 'resize-we',
    'resize-nwse', 'resize-nesw',
  ] as const;

  // Pack slug -> CursorImageSet field suffix
  const packs = [
    { slug: 'screenstudio', suffix: 'ScreenStudio' },
    { slug: 'macos26', suffix: 'Macos26' },
    { slug: 'sgtcute', suffix: 'Sgtcute' },
    { slug: 'sgtcool', suffix: 'Sgtcool' },
    { slug: 'sgtai', suffix: 'Sgtai' },
    { slug: 'sgtpixel', suffix: 'Sgtpixel' },
    { slug: 'jepriwin11', suffix: 'Jepriwin11' },
    { slug: 'sgtwatermelon', suffix: 'Sgtwatermelon' },
    { slug: 'sgtfastfood', suffix: 'Sgtfastfood' },
    { slug: 'sgtveggie', suffix: 'Sgtveggie' },
    { slug: 'sgtvietnam', suffix: 'Sgtvietnam' },
    { slug: 'sgtkorea', suffix: 'Sgtkorea' },
  ] as const;

  // Map cursor type slug -> CursorImageSet field prefix
  const typeFieldPrefix: Record<string, string> = {
    'default': 'default',
    'text': 'text',
    'pointer': 'pointer',
    'openhand': 'openHand',
    'closehand': 'closeHand',
    'wait': 'wait',
    'appstarting': 'appStarting',
    'crosshair': 'crosshair',
    'resize-ns': 'resizeNs',
    'resize-we': 'resizeWe',
    'resize-nwse': 'resizeNwse',
    'resize-nesw': 'resizeNesw',
  };

  const images: Record<string, HTMLImageElement> = {};

  for (const pack of packs) {
    for (const cursorType of types) {
      const prefix = typeFieldPrefix[cursorType];
      const fieldName = `${prefix}${pack.suffix}Image`;
      images[fieldName] = loadImg(`cursor-${cursorType}-${pack.slug}`);
    }
  }

  return images as unknown as CursorImageSet;
}

// ---------------------------------------------------------------------------
// Lazy animation init — only triggered when a project actually needs it
// ---------------------------------------------------------------------------

// Animated cursor types that need preview pre-rendering.
const ANIMATED_CURSOR_TYPES = ['wait', 'appstarting'] as const;

// Raw cursor_type values from recordings that map to animated types.
const ANIMATED_RAW_TYPES = new Set(['wait', 'appstarting']);

const PACK_SUFFIXES: Record<string, string> = {
  screenstudio: 'ScreenStudio',
  macos26: 'Macos26',
  sgtcute: 'Sgtcute',
  sgtcool: 'Sgtcool',
  sgtai: 'Sgtai',
  sgtpixel: 'Sgtpixel',
  jepriwin11: 'Jepriwin11',
  sgtwatermelon: 'Sgtwatermelon',
  sgtfastfood: 'Sgtfastfood',
  sgtveggie: 'Sgtveggie',
  sgtvietnam: 'Sgtvietnam',
  sgtkorea: 'Sgtkorea',
};

const TYPE_FIELD_PREFIX: Record<string, string> = {
  wait: 'wait',
  appstarting: 'appStarting',
};

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
  if (_initedPacks.has(pack)) return;

  const hasAnimated = mousePositions.some(
    p => ANIMATED_RAW_TYPES.has((p.cursor_type || '').toLowerCase()),
  );
  if (!hasAnimated) return;

  _initedPacks.add(pack);

  const suffix = PACK_SUFFIXES[pack];
  if (!suffix) return;

  for (const type of ANIMATED_CURSOR_TYPES) {
    const fieldName = `${TYPE_FIELD_PREFIX[type]}${suffix}Image`;
    const img = (images as unknown as Record<string, HTMLImageElement>)[fieldName];
    if (img) initPreviewAnimation(img, `${type}-${pack}`);
  }
}
