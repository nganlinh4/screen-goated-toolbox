import { CursorImageSet } from './cursorGraphics';

// ---------------------------------------------------------------------------
// Cursor asset version cache-buster
// ---------------------------------------------------------------------------
const CURSOR_ASSET_VERSION = `cursor-types-runtime-${Date.now()}`;

// ---------------------------------------------------------------------------
// createCursorImageSet - loads all cursor pack SVGs into HTMLImageElements
// ---------------------------------------------------------------------------

export function createCursorImageSet(): CursorImageSet {
  const loadImg = (name: string): HTMLImageElement => {
    const img = new Image();
    img.src = `/${name}.svg?v=${CURSOR_ASSET_VERSION}`;
    img.onload = () => { };
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
