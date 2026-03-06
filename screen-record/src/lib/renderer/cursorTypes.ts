import { BackgroundConfig } from '@/types/video';
import {
  CURSOR_VARIANT_FIELDS,
  type CursorImageSet,
  type CursorPack,
  type CursorRenderKind,
  getCursorImageFieldName,
  isCursorPack,
  splitCursorRenderType,
  toCursorRenderType,
} from './cursorModel';

const DRAG_CURSOR_RAW_TYPES = new Set([
  'move',
  'sizeall',
  'drag',
  'dragging',
  'openhand',
  'open-hand',
  'open_hand',
  'closedhand',
  'closed-hand',
  'closed_hand',
  'closehand',
  'close-hand',
  'close_hand',
  'grab',
  'grabbing',
]);

const RAW_CURSOR_KIND_ALIASES: Record<string, CursorRenderKind> = {
  text: 'text',
  ibeam: 'text',
  pointer: 'pointer',
  hand: 'pointer',
  wait: 'wait',
  appstarting: 'appstarting',
  crosshair: 'crosshair',
  cross: 'crosshair',
  resize_ns: 'resize-ns',
  sizens: 'resize-ns',
  resize_we: 'resize-we',
  sizewe: 'resize-we',
  resize_nwse: 'resize-nwse',
  sizenwse: 'resize-nwse',
  resize_nesw: 'resize-nesw',
  sizenesw: 'resize-nesw',
  other: 'default',
  default: 'default',
  arrow: 'default',
};

function resolveCursorKind(rawType: string, isClicked: boolean): CursorRenderKind {
  const lower = (rawType || 'default').toLowerCase();

  if (DRAG_CURSOR_RAW_TYPES.has(lower)) {
    return isClicked ? 'closehand' : 'openhand';
  }

  return RAW_CURSOR_KIND_ALIASES[lower] ?? 'default';
}

export function getCursorPack(backgroundConfig?: BackgroundConfig | null): CursorPack {
  const explicitPack = backgroundConfig?.cursorPack;
  if (explicitPack && isCursorPack(explicitPack)) {
    return explicitPack;
  }

  for (const field of CURSOR_VARIANT_FIELDS) {
    const variant = backgroundConfig?.[field];
    if (variant && isCursorPack(variant) && variant !== 'screenstudio') {
      return variant;
    }
  }

  return 'screenstudio';
}

export function resolveCursorRenderType(
  rawType: string,
  backgroundConfig?: BackgroundConfig | null,
  isClicked: boolean = false,
) {
  return toCursorRenderType(
    getCursorPack(backgroundConfig),
    resolveCursorKind(rawType, isClicked),
  );
}

export function getCursorImage(
  images: CursorImageSet,
  type: ReturnType<typeof resolveCursorRenderType> | string,
): HTMLImageElement | null {
  const parsedType = splitCursorRenderType(type);
  if (!parsedType) {
    return null;
  }

  return images[getCursorImageFieldName(parsedType.pack, parsedType.kind)] ?? null;
}

export {
  CURSOR_PACKS,
  CURSOR_RENDER_KINDS,
  type CursorImageFieldName,
  type CursorImageSet,
  type CursorPack,
  type CursorRenderKind,
  type CursorRenderState,
  type CursorRenderType,
  getCursorImageFieldName,
  isCursorPack,
  splitCursorRenderType,
  toCursorRenderType,
} from './cursorModel';
