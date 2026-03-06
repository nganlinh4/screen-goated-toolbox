export const CURSOR_PACKS = [
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
] as const;

export type CursorPack = (typeof CURSOR_PACKS)[number];

export const CURSOR_RENDER_KINDS = [
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
] as const;

export type CursorRenderKind = (typeof CURSOR_RENDER_KINDS)[number];
export type CursorRenderType = `${CursorRenderKind}-${CursorPack}`;

export const CURSOR_VARIANT_FIELDS = [
  'cursorDefaultVariant',
  'cursorTextVariant',
  'cursorPointerVariant',
  'cursorOpenHandVariant',
] as const;

export type CursorVariantField = (typeof CURSOR_VARIANT_FIELDS)[number];

export const CURSOR_IMAGE_FIELD_BASES = {
  default: 'default',
  text: 'text',
  pointer: 'pointer',
  openhand: 'openHand',
  closehand: 'closeHand',
  wait: 'wait',
  appstarting: 'appStarting',
  crosshair: 'crosshair',
  'resize-ns': 'resizeNs',
  'resize-we': 'resizeWe',
  'resize-nwse': 'resizeNwse',
  'resize-nesw': 'resizeNesw',
} as const satisfies Record<CursorRenderKind, string>;

export const CURSOR_PACK_FIELD_SUFFIXES = {
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
} as const satisfies Record<CursorPack, string>;

type CursorImageFieldPrefix = (typeof CURSOR_IMAGE_FIELD_BASES)[CursorRenderKind];
type CursorPackFieldSuffix = (typeof CURSOR_PACK_FIELD_SUFFIXES)[CursorPack];

export type CursorImageFieldName = `${CursorImageFieldPrefix}${CursorPackFieldSuffix}Image`;
export type CursorImageSet = Record<CursorImageFieldName, HTMLImageElement>;

export interface CursorRenderState {
  cursorOffscreen: OffscreenCanvas;
  cursorOffscreenCtx: OffscreenCanvasRenderingContext2D;
  currentSquishScale: number;
  loggedCursorTypes: Set<string>;
  loggedCursorMappings: Set<string>;
}

const CURSOR_PACK_SET = new Set<CursorPack>(CURSOR_PACKS);
const CURSOR_RENDER_KIND_SET = new Set<CursorRenderKind>(CURSOR_RENDER_KINDS);

export function isCursorPack(value: string): value is CursorPack {
  return CURSOR_PACK_SET.has(value as CursorPack);
}

export function isCursorRenderKind(value: string): value is CursorRenderKind {
  return CURSOR_RENDER_KIND_SET.has(value as CursorRenderKind);
}

export function toCursorRenderType(pack: CursorPack, kind: CursorRenderKind): CursorRenderType {
  return `${kind}-${pack}` as CursorRenderType;
}

export function splitCursorRenderType(
  type: string,
): { pack: CursorPack; kind: CursorRenderKind } | null {
  for (const pack of CURSOR_PACKS) {
    const packSuffix = `-${pack}`;
    if (!type.endsWith(packSuffix)) {
      continue;
    }

    const kind = type.slice(0, -packSuffix.length);
    if (isCursorRenderKind(kind)) {
      return { pack, kind };
    }
  }

  return null;
}

export function getCursorImageFieldName(
  pack: CursorPack,
  kind: CursorRenderKind,
): CursorImageFieldName {
  return `${CURSOR_IMAGE_FIELD_BASES[kind]}${CURSOR_PACK_FIELD_SUFFIXES[pack]}Image` as CursorImageFieldName;
}
