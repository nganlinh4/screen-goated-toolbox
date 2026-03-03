import { BackgroundConfig } from '@/types/video';

// ---------------------------------------------------------------------------
// CursorRenderType – union of all pack-qualified cursor identifiers
// ---------------------------------------------------------------------------

export type CursorRenderType =
  | 'default-screenstudio'
  | 'text-screenstudio'
  | 'pointer-screenstudio'
  | 'openhand-screenstudio'
  | 'closehand-screenstudio'
  | 'wait-screenstudio'
  | 'appstarting-screenstudio'
  | 'crosshair-screenstudio'
  | 'resize-ns-screenstudio'
  | 'resize-we-screenstudio'
  | 'resize-nwse-screenstudio'
  | 'resize-nesw-screenstudio'
  | 'default-macos26'
  | 'text-macos26'
  | 'pointer-macos26'
  | 'openhand-macos26'
  | 'closehand-macos26'
  | 'wait-macos26'
  | 'appstarting-macos26'
  | 'crosshair-macos26'
  | 'resize-ns-macos26'
  | 'resize-we-macos26'
  | 'resize-nwse-macos26'
  | 'resize-nesw-macos26'
  | 'default-sgtcute'
  | 'text-sgtcute'
  | 'pointer-sgtcute'
  | 'openhand-sgtcute'
  | 'closehand-sgtcute'
  | 'wait-sgtcute'
  | 'appstarting-sgtcute'
  | 'crosshair-sgtcute'
  | 'resize-ns-sgtcute'
  | 'resize-we-sgtcute'
  | 'resize-nwse-sgtcute'
  | 'resize-nesw-sgtcute'
  | 'default-sgtcool'
  | 'text-sgtcool'
  | 'pointer-sgtcool'
  | 'openhand-sgtcool'
  | 'closehand-sgtcool'
  | 'wait-sgtcool'
  | 'appstarting-sgtcool'
  | 'crosshair-sgtcool'
  | 'resize-ns-sgtcool'
  | 'resize-we-sgtcool'
  | 'resize-nwse-sgtcool'
  | 'resize-nesw-sgtcool'
  | 'default-sgtai'
  | 'text-sgtai'
  | 'pointer-sgtai'
  | 'openhand-sgtai'
  | 'closehand-sgtai'
  | 'wait-sgtai'
  | 'appstarting-sgtai'
  | 'crosshair-sgtai'
  | 'resize-ns-sgtai'
  | 'resize-we-sgtai'
  | 'resize-nwse-sgtai'
  | 'resize-nesw-sgtai'
  | 'default-sgtpixel'
  | 'text-sgtpixel'
  | 'pointer-sgtpixel'
  | 'openhand-sgtpixel'
  | 'closehand-sgtpixel'
  | 'wait-sgtpixel'
  | 'appstarting-sgtpixel'
  | 'crosshair-sgtpixel'
  | 'resize-ns-sgtpixel'
  | 'resize-we-sgtpixel'
  | 'resize-nwse-sgtpixel'
  | 'resize-nesw-sgtpixel'
  | 'default-jepriwin11'
  | 'text-jepriwin11'
  | 'pointer-jepriwin11'
  | 'openhand-jepriwin11'
  | 'closehand-jepriwin11'
  | 'wait-jepriwin11'
  | 'appstarting-jepriwin11'
  | 'crosshair-jepriwin11'
  | 'resize-ns-jepriwin11'
  | 'resize-we-jepriwin11'
  | 'resize-nwse-jepriwin11'
  | 'resize-nesw-jepriwin11'
  | 'default-sgtwatermelon'
  | 'text-sgtwatermelon'
  | 'pointer-sgtwatermelon'
  | 'openhand-sgtwatermelon'
  | 'closehand-sgtwatermelon'
  | 'wait-sgtwatermelon'
  | 'appstarting-sgtwatermelon'
  | 'crosshair-sgtwatermelon'
  | 'resize-ns-sgtwatermelon'
  | 'resize-we-sgtwatermelon'
  | 'resize-nwse-sgtwatermelon'
  | 'resize-nesw-sgtwatermelon';

// ---------------------------------------------------------------------------
// CursorImageSet – all HTMLImageElement fields used by cursor rendering
// ---------------------------------------------------------------------------

export interface CursorImageSet {
  // ScreenStudio pack
  pointerScreenStudioImage: HTMLImageElement;
  defaultScreenStudioImage: HTMLImageElement;
  textScreenStudioImage: HTMLImageElement;
  openHandScreenStudioImage: HTMLImageElement;
  closeHandScreenStudioImage: HTMLImageElement;
  waitScreenStudioImage: HTMLImageElement;
  appStartingScreenStudioImage: HTMLImageElement;
  crosshairScreenStudioImage: HTMLImageElement;
  resizeNsScreenStudioImage: HTMLImageElement;
  resizeWeScreenStudioImage: HTMLImageElement;
  resizeNwseScreenStudioImage: HTMLImageElement;
  resizeNeswScreenStudioImage: HTMLImageElement;

  // macOS 26 pack
  defaultMacos26Image: HTMLImageElement;
  textMacos26Image: HTMLImageElement;
  pointerMacos26Image: HTMLImageElement;
  openHandMacos26Image: HTMLImageElement;
  closeHandMacos26Image: HTMLImageElement;
  waitMacos26Image: HTMLImageElement;
  appStartingMacos26Image: HTMLImageElement;
  crosshairMacos26Image: HTMLImageElement;
  resizeNsMacos26Image: HTMLImageElement;
  resizeWeMacos26Image: HTMLImageElement;
  resizeNwseMacos26Image: HTMLImageElement;
  resizeNeswMacos26Image: HTMLImageElement;

  // SGT Cute pack
  defaultSgtcuteImage: HTMLImageElement;
  textSgtcuteImage: HTMLImageElement;
  pointerSgtcuteImage: HTMLImageElement;
  openHandSgtcuteImage: HTMLImageElement;
  closeHandSgtcuteImage: HTMLImageElement;
  waitSgtcuteImage: HTMLImageElement;
  appStartingSgtcuteImage: HTMLImageElement;
  crosshairSgtcuteImage: HTMLImageElement;
  resizeNsSgtcuteImage: HTMLImageElement;
  resizeWeSgtcuteImage: HTMLImageElement;
  resizeNwseSgtcuteImage: HTMLImageElement;
  resizeNeswSgtcuteImage: HTMLImageElement;

  // SGT Cool pack
  defaultSgtcoolImage: HTMLImageElement;
  textSgtcoolImage: HTMLImageElement;
  pointerSgtcoolImage: HTMLImageElement;
  openHandSgtcoolImage: HTMLImageElement;
  closeHandSgtcoolImage: HTMLImageElement;
  waitSgtcoolImage: HTMLImageElement;
  appStartingSgtcoolImage: HTMLImageElement;
  crosshairSgtcoolImage: HTMLImageElement;
  resizeNsSgtcoolImage: HTMLImageElement;
  resizeWeSgtcoolImage: HTMLImageElement;
  resizeNwseSgtcoolImage: HTMLImageElement;
  resizeNeswSgtcoolImage: HTMLImageElement;

  // SGT AI pack
  defaultSgtaiImage: HTMLImageElement;
  textSgtaiImage: HTMLImageElement;
  pointerSgtaiImage: HTMLImageElement;
  openHandSgtaiImage: HTMLImageElement;
  closeHandSgtaiImage: HTMLImageElement;
  waitSgtaiImage: HTMLImageElement;
  appStartingSgtaiImage: HTMLImageElement;
  crosshairSgtaiImage: HTMLImageElement;
  resizeNsSgtaiImage: HTMLImageElement;
  resizeWeSgtaiImage: HTMLImageElement;
  resizeNwseSgtaiImage: HTMLImageElement;
  resizeNeswSgtaiImage: HTMLImageElement;

  // SGT Pixel pack
  defaultSgtpixelImage: HTMLImageElement;
  textSgtpixelImage: HTMLImageElement;
  pointerSgtpixelImage: HTMLImageElement;
  openHandSgtpixelImage: HTMLImageElement;
  closeHandSgtpixelImage: HTMLImageElement;
  waitSgtpixelImage: HTMLImageElement;
  appStartingSgtpixelImage: HTMLImageElement;
  crosshairSgtpixelImage: HTMLImageElement;
  resizeNsSgtpixelImage: HTMLImageElement;
  resizeWeSgtpixelImage: HTMLImageElement;
  resizeNwseSgtpixelImage: HTMLImageElement;
  resizeNeswSgtpixelImage: HTMLImageElement;

  // Jepriwin11 pack
  defaultJepriwin11Image: HTMLImageElement;
  textJepriwin11Image: HTMLImageElement;
  pointerJepriwin11Image: HTMLImageElement;
  openHandJepriwin11Image: HTMLImageElement;
  closeHandJepriwin11Image: HTMLImageElement;
  waitJepriwin11Image: HTMLImageElement;
  appStartingJepriwin11Image: HTMLImageElement;
  crosshairJepriwin11Image: HTMLImageElement;
  resizeNsJepriwin11Image: HTMLImageElement;
  resizeWeJepriwin11Image: HTMLImageElement;
  resizeNwseJepriwin11Image: HTMLImageElement;
  resizeNeswJepriwin11Image: HTMLImageElement;

  // SGT Watermelon pack
  defaultSgtwatermelonImage: HTMLImageElement;
  textSgtwatermelonImage: HTMLImageElement;
  pointerSgtwatermelonImage: HTMLImageElement;
  openHandSgtwatermelonImage: HTMLImageElement;
  closeHandSgtwatermelonImage: HTMLImageElement;
  waitSgtwatermelonImage: HTMLImageElement;
  appStartingSgtwatermelonImage: HTMLImageElement;
  crosshairSgtwatermelonImage: HTMLImageElement;
  resizeNsSgtwatermelonImage: HTMLImageElement;
  resizeWeSgtwatermelonImage: HTMLImageElement;
  resizeNwseSgtwatermelonImage: HTMLImageElement;
  resizeNeswSgtwatermelonImage: HTMLImageElement;
}

// ---------------------------------------------------------------------------
// CursorRenderState – mutable state passed from VideoRenderer into draw calls
// ---------------------------------------------------------------------------

export interface CursorRenderState {
  cursorOffscreen: OffscreenCanvas;
  cursorOffscreenCtx: OffscreenCanvasRenderingContext2D;
  currentSquishScale: number;
  loggedCursorTypes: Set<string>;
  loggedCursorMappings: Set<string>;
}

// ---------------------------------------------------------------------------
// getCursorPack – determine which cursor pack is active
// ---------------------------------------------------------------------------

export function getCursorPack(backgroundConfig?: BackgroundConfig | null): 'screenstudio' | 'macos26' | 'sgtcute' | 'sgtcool' | 'sgtai' | 'sgtpixel' | 'jepriwin11' | 'sgtwatermelon' {
  if (backgroundConfig?.cursorPack === 'sgtwatermelon') return 'sgtwatermelon';
  if (backgroundConfig?.cursorPack === 'jepriwin11') return 'jepriwin11';
  if (backgroundConfig?.cursorPack === 'sgtpixel') return 'sgtpixel';
  if (backgroundConfig?.cursorPack === 'sgtai') return 'sgtai';
  if (backgroundConfig?.cursorPack === 'sgtcool') return 'sgtcool';
  if (backgroundConfig?.cursorPack === 'sgtcute') return 'sgtcute';
  if (backgroundConfig?.cursorPack === 'macos26') return 'macos26';
  if (backgroundConfig?.cursorPack === 'screenstudio') return 'screenstudio';
  if (backgroundConfig?.cursorDefaultVariant === 'sgtwatermelon'
    || backgroundConfig?.cursorTextVariant === 'sgtwatermelon'
    || backgroundConfig?.cursorPointerVariant === 'sgtwatermelon'
    || backgroundConfig?.cursorOpenHandVariant === 'sgtwatermelon') {
    return 'sgtwatermelon';
  }
  if (backgroundConfig?.cursorDefaultVariant === 'jepriwin11'
    || backgroundConfig?.cursorTextVariant === 'jepriwin11'
    || backgroundConfig?.cursorPointerVariant === 'jepriwin11'
    || backgroundConfig?.cursorOpenHandVariant === 'jepriwin11') {
    return 'jepriwin11';
  }
  if (backgroundConfig?.cursorDefaultVariant === 'sgtpixel'
    || backgroundConfig?.cursorTextVariant === 'sgtpixel'
    || backgroundConfig?.cursorPointerVariant === 'sgtpixel'
    || backgroundConfig?.cursorOpenHandVariant === 'sgtpixel') {
    return 'sgtpixel';
  }
  if (backgroundConfig?.cursorDefaultVariant === 'sgtai'
    || backgroundConfig?.cursorTextVariant === 'sgtai'
    || backgroundConfig?.cursorPointerVariant === 'sgtai'
    || backgroundConfig?.cursorOpenHandVariant === 'sgtai') {
    return 'sgtai';
  }
  if (backgroundConfig?.cursorDefaultVariant === 'sgtcool'
    || backgroundConfig?.cursorTextVariant === 'sgtcool'
    || backgroundConfig?.cursorPointerVariant === 'sgtcool'
    || backgroundConfig?.cursorOpenHandVariant === 'sgtcool') {
    return 'sgtcool';
  }
  if (backgroundConfig?.cursorDefaultVariant === 'sgtcute'
    || backgroundConfig?.cursorTextVariant === 'sgtcute'
    || backgroundConfig?.cursorPointerVariant === 'sgtcute'
    || backgroundConfig?.cursorOpenHandVariant === 'sgtcute') {
    return 'sgtcute';
  }
  if (backgroundConfig?.cursorDefaultVariant === 'macos26'
    || backgroundConfig?.cursorTextVariant === 'macos26'
    || backgroundConfig?.cursorPointerVariant === 'macos26'
    || backgroundConfig?.cursorOpenHandVariant === 'macos26') {
    return 'macos26';
  }
  return 'screenstudio';
}

// ---------------------------------------------------------------------------
// resolveCursorRenderType – map raw cursor type string to a CursorRenderType
// ---------------------------------------------------------------------------

export function resolveCursorRenderType(rawType: string, backgroundConfig?: BackgroundConfig | null, isClicked: boolean = false): CursorRenderType {
  const lower = (rawType || 'default').toLowerCase();
  const pack = getCursorPack(backgroundConfig);

  const semanticType =
    (lower === 'text' || lower === 'ibeam') ? 'text'
      : (lower === 'pointer' || lower === 'hand') ? 'pointer'
        : (lower === 'wait') ? 'wait'
          : (lower === 'appstarting') ? 'appstarting'
            : (lower === 'crosshair' || lower === 'cross') ? 'crosshair'
              : (lower === 'resize_ns' || lower === 'sizens') ? 'resize_ns'
                : (lower === 'resize_we' || lower === 'sizewe') ? 'resize_we'
                  : (lower === 'resize_nwse' || lower === 'sizenwse') ? 'resize_nwse'
                    : (lower === 'resize_nesw' || lower === 'sizenesw') ? 'resize_nesw'
                      : (
                        lower === 'move' ||
                        lower === 'sizeall' ||
                        lower === 'drag' ||
                        lower === 'dragging' ||
                        lower === 'openhand' ||
                        lower === 'open-hand' ||
                        lower === 'open_hand' ||
                        lower === 'closedhand' ||
                        lower === 'closed-hand' ||
                        lower === 'closed_hand' ||
                        lower === 'closehand' ||
                        lower === 'close-hand' ||
                        lower === 'close_hand' ||
                        lower === 'grab' ||
                        lower === 'grabbing'
                      )
                        ? (isClicked ? 'closehand' : 'openhand')
                        : (lower === 'other') ? 'default'
                          : (lower === 'default' || lower === 'arrow') ? 'default'
                          : 'default';

  if (pack === 'macos26') {
    switch (semanticType) {
      case 'text': return 'text-macos26';
      case 'pointer': return 'pointer-macos26';
      case 'openhand': return 'openhand-macos26';
      case 'closehand': return 'closehand-macos26';
      case 'wait': return 'wait-macos26';
      case 'appstarting': return 'appstarting-macos26';
      case 'crosshair': return 'crosshair-macos26';
      case 'resize_ns': return 'resize-ns-macos26';
      case 'resize_we': return 'resize-we-macos26';
      case 'resize_nwse': return 'resize-nwse-macos26';
      case 'resize_nesw': return 'resize-nesw-macos26';
      default: return 'default-macos26';
    }
  }

  if (pack === 'sgtcool') {
    switch (semanticType) {
      case 'text': return 'text-sgtcool';
      case 'pointer': return 'pointer-sgtcool';
      case 'openhand': return 'openhand-sgtcool';
      case 'closehand': return 'closehand-sgtcool';
      case 'wait': return 'wait-sgtcool';
      case 'appstarting': return 'appstarting-sgtcool';
      case 'crosshair': return 'crosshair-sgtcool';
      case 'resize_ns': return 'resize-ns-sgtcool';
      case 'resize_we': return 'resize-we-sgtcool';
      case 'resize_nwse': return 'resize-nwse-sgtcool';
      case 'resize_nesw': return 'resize-nesw-sgtcool';
      default: return 'default-sgtcool';
    }
  }

  if (pack === 'sgtai') {
    switch (semanticType) {
      case 'text': return 'text-sgtai';
      case 'pointer': return 'pointer-sgtai';
      case 'openhand': return 'openhand-sgtai';
      case 'closehand': return 'closehand-sgtai';
      case 'wait': return 'wait-sgtai';
      case 'appstarting': return 'appstarting-sgtai';
      case 'crosshair': return 'crosshair-sgtai';
      case 'resize_ns': return 'resize-ns-sgtai';
      case 'resize_we': return 'resize-we-sgtai';
      case 'resize_nwse': return 'resize-nwse-sgtai';
      case 'resize_nesw': return 'resize-nesw-sgtai';
      default: return 'default-sgtai';
    }
  }

  if (pack === 'sgtpixel') {
    switch (semanticType) {
      case 'text': return 'text-sgtpixel';
      case 'pointer': return 'pointer-sgtpixel';
      case 'openhand': return 'openhand-sgtpixel';
      case 'closehand': return 'closehand-sgtpixel';
      case 'wait': return 'wait-sgtpixel';
      case 'appstarting': return 'appstarting-sgtpixel';
      case 'crosshair': return 'crosshair-sgtpixel';
      case 'resize_ns': return 'resize-ns-sgtpixel';
      case 'resize_we': return 'resize-we-sgtpixel';
      case 'resize_nwse': return 'resize-nwse-sgtpixel';
      case 'resize_nesw': return 'resize-nesw-sgtpixel';
      default: return 'default-sgtpixel';
    }
  }

  if (pack === 'sgtwatermelon') {
    switch (semanticType) {
      case 'text': return 'text-sgtwatermelon';
      case 'pointer': return 'pointer-sgtwatermelon';
      case 'openhand': return 'openhand-sgtwatermelon';
      case 'closehand': return 'closehand-sgtwatermelon';
      case 'wait': return 'wait-sgtwatermelon';
      case 'appstarting': return 'appstarting-sgtwatermelon';
      case 'crosshair': return 'crosshair-sgtwatermelon';
      case 'resize_ns': return 'resize-ns-sgtwatermelon';
      case 'resize_we': return 'resize-we-sgtwatermelon';
      case 'resize_nwse': return 'resize-nwse-sgtwatermelon';
      case 'resize_nesw': return 'resize-nesw-sgtwatermelon';
      default: return 'default-sgtwatermelon';
    }
  }

  if (pack === 'jepriwin11') {
    switch (semanticType) {
      case 'text': return 'text-jepriwin11';
      case 'pointer': return 'pointer-jepriwin11';
      case 'openhand': return 'openhand-jepriwin11';
      case 'closehand': return 'closehand-jepriwin11';
      case 'wait': return 'wait-jepriwin11';
      case 'appstarting': return 'appstarting-jepriwin11';
      case 'crosshair': return 'crosshair-jepriwin11';
      case 'resize_ns': return 'resize-ns-jepriwin11';
      case 'resize_we': return 'resize-we-jepriwin11';
      case 'resize_nwse': return 'resize-nwse-jepriwin11';
      case 'resize_nesw': return 'resize-nesw-jepriwin11';
      default: return 'default-jepriwin11';
    }
  }

  if (pack === 'sgtcute') {
    switch (semanticType) {
      case 'text': return 'text-sgtcute';
      case 'pointer': return 'pointer-sgtcute';
      case 'openhand': return 'openhand-sgtcute';
      case 'closehand': return 'closehand-sgtcute';
      case 'wait': return 'wait-sgtcute';
      case 'appstarting': return 'appstarting-sgtcute';
      case 'crosshair': return 'crosshair-sgtcute';
      case 'resize_ns': return 'resize-ns-sgtcute';
      case 'resize_we': return 'resize-we-sgtcute';
      case 'resize_nwse': return 'resize-nwse-sgtcute';
      case 'resize_nesw': return 'resize-nesw-sgtcute';
      default: return 'default-sgtcute';
    }
  }

  switch (semanticType) {
    case 'text': return 'text-screenstudio';
    case 'pointer': return 'pointer-screenstudio';
    case 'openhand': return 'openhand-screenstudio';
    case 'closehand': return 'closehand-screenstudio';
    case 'wait': return 'wait-screenstudio';
    case 'appstarting': return 'appstarting-screenstudio';
    case 'crosshair': return 'crosshair-screenstudio';
    case 'resize_ns': return 'resize-ns-screenstudio';
    case 'resize_we': return 'resize-we-screenstudio';
    case 'resize_nwse': return 'resize-nwse-screenstudio';
    case 'resize_nesw': return 'resize-nesw-screenstudio';
    default: return 'default-screenstudio';
  }
}

// ---------------------------------------------------------------------------
// Per-pack image resolvers
// ---------------------------------------------------------------------------

export function getMacos26CursorImage(images: CursorImageSet, type: CursorRenderType): HTMLImageElement | null {
  switch (type) {
    case 'default-macos26': return images.defaultMacos26Image;
    case 'text-macos26': return images.textMacos26Image;
    case 'pointer-macos26': return images.pointerMacos26Image;
    case 'openhand-macos26': return images.openHandMacos26Image;
    case 'closehand-macos26': return images.closeHandMacos26Image;
    case 'wait-macos26': return images.waitMacos26Image;
    case 'appstarting-macos26': return images.appStartingMacos26Image;
    case 'crosshair-macos26': return images.crosshairMacos26Image;
    case 'resize-ns-macos26': return images.resizeNsMacos26Image;
    case 'resize-we-macos26': return images.resizeWeMacos26Image;
    case 'resize-nwse-macos26': return images.resizeNwseMacos26Image;
    case 'resize-nesw-macos26': return images.resizeNeswMacos26Image;
    default: return null;
  }
}

export function getSgtcuteCursorImage(images: CursorImageSet, type: CursorRenderType): HTMLImageElement | null {
  switch (type) {
    case 'default-sgtcute': return images.defaultSgtcuteImage;
    case 'text-sgtcute': return images.textSgtcuteImage;
    case 'pointer-sgtcute': return images.pointerSgtcuteImage;
    case 'openhand-sgtcute': return images.openHandSgtcuteImage;
    case 'closehand-sgtcute': return images.closeHandSgtcuteImage;
    case 'wait-sgtcute': return images.waitSgtcuteImage;
    case 'appstarting-sgtcute': return images.appStartingSgtcuteImage;
    case 'crosshair-sgtcute': return images.crosshairSgtcuteImage;
    case 'resize-ns-sgtcute': return images.resizeNsSgtcuteImage;
    case 'resize-we-sgtcute': return images.resizeWeSgtcuteImage;
    case 'resize-nwse-sgtcute': return images.resizeNwseSgtcuteImage;
    case 'resize-nesw-sgtcute': return images.resizeNeswSgtcuteImage;
    default: return null;
  }
}

export function getSgtcoolCursorImage(images: CursorImageSet, type: CursorRenderType): HTMLImageElement | null {
  switch (type) {
    case 'default-sgtcool': return images.defaultSgtcoolImage;
    case 'text-sgtcool': return images.textSgtcoolImage;
    case 'pointer-sgtcool': return images.pointerSgtcoolImage;
    case 'openhand-sgtcool': return images.openHandSgtcoolImage;
    case 'closehand-sgtcool': return images.closeHandSgtcoolImage;
    case 'wait-sgtcool': return images.waitSgtcoolImage;
    case 'appstarting-sgtcool': return images.appStartingSgtcoolImage;
    case 'crosshair-sgtcool': return images.crosshairSgtcoolImage;
    case 'resize-ns-sgtcool': return images.resizeNsSgtcoolImage;
    case 'resize-we-sgtcool': return images.resizeWeSgtcoolImage;
    case 'resize-nwse-sgtcool': return images.resizeNwseSgtcoolImage;
    case 'resize-nesw-sgtcool': return images.resizeNeswSgtcoolImage;
    default: return null;
  }
}

export function getSgtaiCursorImage(images: CursorImageSet, type: CursorRenderType): HTMLImageElement | null {
  switch (type) {
    case 'default-sgtai': return images.defaultSgtaiImage;
    case 'text-sgtai': return images.textSgtaiImage;
    case 'pointer-sgtai': return images.pointerSgtaiImage;
    case 'openhand-sgtai': return images.openHandSgtaiImage;
    case 'closehand-sgtai': return images.closeHandSgtaiImage;
    case 'wait-sgtai': return images.waitSgtaiImage;
    case 'appstarting-sgtai': return images.appStartingSgtaiImage;
    case 'crosshair-sgtai': return images.crosshairSgtaiImage;
    case 'resize-ns-sgtai': return images.resizeNsSgtaiImage;
    case 'resize-we-sgtai': return images.resizeWeSgtaiImage;
    case 'resize-nwse-sgtai': return images.resizeNwseSgtaiImage;
    case 'resize-nesw-sgtai': return images.resizeNeswSgtaiImage;
    default: return null;
  }
}

export function getSgtpixelCursorImage(images: CursorImageSet, type: CursorRenderType): HTMLImageElement | null {
  switch (type) {
    case 'default-sgtpixel': return images.defaultSgtpixelImage;
    case 'text-sgtpixel': return images.textSgtpixelImage;
    case 'pointer-sgtpixel': return images.pointerSgtpixelImage;
    case 'openhand-sgtpixel': return images.openHandSgtpixelImage;
    case 'closehand-sgtpixel': return images.closeHandSgtpixelImage;
    case 'wait-sgtpixel': return images.waitSgtpixelImage;
    case 'appstarting-sgtpixel': return images.appStartingSgtpixelImage;
    case 'crosshair-sgtpixel': return images.crosshairSgtpixelImage;
    case 'resize-ns-sgtpixel': return images.resizeNsSgtpixelImage;
    case 'resize-we-sgtpixel': return images.resizeWeSgtpixelImage;
    case 'resize-nwse-sgtpixel': return images.resizeNwseSgtpixelImage;
    case 'resize-nesw-sgtpixel': return images.resizeNeswSgtpixelImage;
    default: return null;
  }
}

export function getJepriwin11CursorImage(images: CursorImageSet, type: CursorRenderType): HTMLImageElement | null {
  switch (type) {
    case 'default-jepriwin11': return images.defaultJepriwin11Image;
    case 'text-jepriwin11': return images.textJepriwin11Image;
    case 'pointer-jepriwin11': return images.pointerJepriwin11Image;
    case 'openhand-jepriwin11': return images.openHandJepriwin11Image;
    case 'closehand-jepriwin11': return images.closeHandJepriwin11Image;
    case 'wait-jepriwin11': return images.waitJepriwin11Image;
    case 'appstarting-jepriwin11': return images.appStartingJepriwin11Image;
    case 'crosshair-jepriwin11': return images.crosshairJepriwin11Image;
    case 'resize-ns-jepriwin11': return images.resizeNsJepriwin11Image;
    case 'resize-we-jepriwin11': return images.resizeWeJepriwin11Image;
    case 'resize-nwse-jepriwin11': return images.resizeNwseJepriwin11Image;
    case 'resize-nesw-jepriwin11': return images.resizeNeswJepriwin11Image;
    default: return null;
  }
}

export function getSgtwatermelonCursorImage(images: CursorImageSet, type: CursorRenderType): HTMLImageElement | null {
  switch (type) {
    case 'default-sgtwatermelon': return images.defaultSgtwatermelonImage;
    case 'text-sgtwatermelon': return images.textSgtwatermelonImage;
    case 'pointer-sgtwatermelon': return images.pointerSgtwatermelonImage;
    case 'openhand-sgtwatermelon': return images.openHandSgtwatermelonImage;
    case 'closehand-sgtwatermelon': return images.closeHandSgtwatermelonImage;
    case 'wait-sgtwatermelon': return images.waitSgtwatermelonImage;
    case 'appstarting-sgtwatermelon': return images.appStartingSgtwatermelonImage;
    case 'crosshair-sgtwatermelon': return images.crosshairSgtwatermelonImage;
    case 'resize-ns-sgtwatermelon': return images.resizeNsSgtwatermelonImage;
    case 'resize-we-sgtwatermelon': return images.resizeWeSgtwatermelonImage;
    case 'resize-nwse-sgtwatermelon': return images.resizeNwseSgtwatermelonImage;
    case 'resize-nesw-sgtwatermelon': return images.resizeNeswSgtwatermelonImage;
    default: return null;
  }
}

export function getScreenStudioCursorImage(images: CursorImageSet, type: CursorRenderType | string): HTMLImageElement | null {
  switch (type) {
    case 'default-screenstudio': return images.defaultScreenStudioImage;
    case 'text-screenstudio': return images.textScreenStudioImage;
    case 'pointer-screenstudio': return images.pointerScreenStudioImage;
    case 'openhand-screenstudio': return images.openHandScreenStudioImage;
    case 'closehand-screenstudio': return images.closeHandScreenStudioImage;
    case 'wait-screenstudio': return images.waitScreenStudioImage;
    case 'appstarting-screenstudio': return images.appStartingScreenStudioImage;
    case 'crosshair-screenstudio': return images.crosshairScreenStudioImage;
    case 'resize-ns-screenstudio': return images.resizeNsScreenStudioImage;
    case 'resize-we-screenstudio': return images.resizeWeScreenStudioImage;
    case 'resize-nwse-screenstudio': return images.resizeNwseScreenStudioImage;
    case 'resize-nesw-screenstudio': return images.resizeNeswScreenStudioImage;
    default: return null;
  }
}
