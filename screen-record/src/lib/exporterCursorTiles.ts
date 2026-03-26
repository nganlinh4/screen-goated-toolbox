import type { BackgroundConfig } from '@/types/video';
import { getCursorPack } from './renderer/cursorTypes';
import { getCursorAssetUrl } from './renderer/cursorAssets';
import { invoke } from '@/lib/ipc';

export type CursorPackSlug =
  | 'screenstudio'
  | 'macos26'
  | 'sgtcute'
  | 'sgtcool'
  | 'sgtai'
  | 'sgtpixel'
  | 'jepriwin11'
  | 'sgtwatermelon'
  | 'sgtfastfood'
  | 'sgtveggie'
  | 'sgtvietnam'
  | 'sgtkorea';

export const CURSOR_TYPES_ORDER = [
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
  'resize-nesw'
] as const;

export const CURSOR_PACK_ORDER: CursorPackSlug[] = [
  'screenstudio',
  'macos26',
  'sgtcute',
  'sgtcool',
  'sgtai',
  'sgtpixel',
  'jepriwin11',
  'sgtwatermelon',
  'sgtfastfood',
  'sgtveggie',
  'sgtvietnam',
  'sgtkorea',
];

export const CURSOR_TILE_SIZE = 512;

export interface CursorSlotPngPayload {
  slotId: number;
  pngBase64: string;
}

function loadImage(src: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => resolve(img);
    img.onerror = () => reject(new Error(`Failed to load ${src}`));
    img.src = src;
  });
}

export function buildCursorSlotId(pack: CursorPackSlug, typeIndex: number): number {
  const packIndex = CURSOR_PACK_ORDER.indexOf(pack);
  if (packIndex < 0) return -1;
  return (packIndex * CURSOR_TYPES_ORDER.length) + typeIndex;
}

export async function buildCursorSlotTilePayload(
  pack: CursorPackSlug,
  typeName: typeof CURSOR_TYPES_ORDER[number],
  typeIndex: number,
): Promise<CursorSlotPngPayload | null> {
  const slotId = buildCursorSlotId(pack, typeIndex);
  if (slotId < 0) return null;

  const src = getCursorAssetUrl(`cursor-${typeName}-${pack}`);
  let img: HTMLImageElement;
  try {
    img = await loadImage(src);
  } catch {
    return null;
  }

  if (!img.complete || img.naturalWidth <= 0 || img.naturalHeight <= 0) {
    return null;
  }

  const sourceMax = Math.max(img.naturalWidth, img.naturalHeight);
  const normalizeScale = sourceMax > 96 ? (48 / sourceMax) : 1;
  const drawW = img.naturalWidth * normalizeScale;
  const drawH = img.naturalHeight * normalizeScale;
  const targetMax = Math.max(drawW, drawH);
  if (targetMax <= 0.0001) {
    return null;
  }

  const tileCanvas = document.createElement('canvas');
  tileCanvas.width = CURSOR_TILE_SIZE;
  tileCanvas.height = CURSOR_TILE_SIZE;
  const tileCtx = tileCanvas.getContext('2d');
  if (!tileCtx) {
    return null;
  }
  tileCtx.clearRect(0, 0, CURSOR_TILE_SIZE, CURSOR_TILE_SIZE);
  tileCtx.imageSmoothingEnabled = true;
  tileCtx.imageSmoothingQuality = 'high';

  const tileScale = CURSOR_TILE_SIZE / targetMax;
  const tileW = drawW * tileScale;
  const tileH = drawH * tileScale;
  const x = (CURSOR_TILE_SIZE - tileW) * 0.5;
  const y = (CURSOR_TILE_SIZE - tileH) * 0.5;
  tileCtx.drawImage(img, x, y, tileW, tileH);

  return {
    slotId,
    pngBase64: tileCanvas.toDataURL('image/png'),
  };
}

export async function stageBrowserCursorSlotTiles(backgroundConfig?: BackgroundConfig) {
  const pack = getCursorPack(backgroundConfig) as CursorPackSlug;
  const staged = (await Promise.all(
    CURSOR_TYPES_ORDER.map((typeName, idx) =>
      buildCursorSlotTilePayload(pack, typeName, idx))
  )).filter((payload): payload is CursorSlotPngPayload => payload !== null);
  if (staged.length === 0) return;
  await invoke('stage_export_data', {
    dataType: 'cursor_slots_png',
    data: staged,
  });
}
