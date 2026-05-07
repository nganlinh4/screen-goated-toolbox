import type { BackgroundConfig } from "@/types/video";
import { DEFAULT_BACKGROUND_CONFIG } from "@/lib/appUtils";

function finiteOr<T extends number | undefined>(
  value: number | null | undefined,
  fallback: T,
): number | T {
  return typeof value === "number" && Number.isFinite(value) ? value : fallback;
}

function optionalFinite(
  value: number | null | undefined,
): number | undefined {
  return typeof value === "number" && Number.isFinite(value) ? value : undefined;
}

export function normalizeBackgroundConfig(
  backgroundConfig: Partial<BackgroundConfig> | null | undefined,
): BackgroundConfig {
  const parsed = backgroundConfig ?? {};
  return {
    ...DEFAULT_BACKGROUND_CONFIG,
    ...parsed,
    scale: finiteOr(parsed.scale, DEFAULT_BACKGROUND_CONFIG.scale),
    borderRadius: finiteOr(
      parsed.borderRadius,
      DEFAULT_BACKGROUND_CONFIG.borderRadius,
    ),
    backgroundType:
      typeof parsed.backgroundType === "string"
        ? parsed.backgroundType
        : DEFAULT_BACKGROUND_CONFIG.backgroundType,
    shadow: finiteOr(parsed.shadow, DEFAULT_BACKGROUND_CONFIG.shadow ?? 0),
    cursorScale: finiteOr(
      parsed.cursorScale,
      DEFAULT_BACKGROUND_CONFIG.cursorScale ?? 5,
    ),
    cursorShadow: finiteOr(
      parsed.cursorShadow,
      DEFAULT_BACKGROUND_CONFIG.cursorShadow ?? 0,
    ),
    cursorMovementDelay: finiteOr(
      parsed.cursorMovementDelay,
      DEFAULT_BACKGROUND_CONFIG.cursorMovementDelay ?? 0,
    ),
    cursorWiggleStrength: optionalFinite(parsed.cursorWiggleStrength),
    cursorWiggleDamping: optionalFinite(parsed.cursorWiggleDamping),
    cursorWiggleResponse: optionalFinite(parsed.cursorWiggleResponse),
    cursorTiltAngle: optionalFinite(parsed.cursorTiltAngle),
    motionBlurCursor: finiteOr(
      parsed.motionBlurCursor,
      DEFAULT_BACKGROUND_CONFIG.motionBlurCursor ?? 0,
    ),
    motionBlurZoom: finiteOr(
      parsed.motionBlurZoom,
      DEFAULT_BACKGROUND_CONFIG.motionBlurZoom ?? 0,
    ),
    motionBlurPan: finiteOr(
      parsed.motionBlurPan,
      DEFAULT_BACKGROUND_CONFIG.motionBlurPan ?? 0,
    ),
    backgroundZoomWithVideo:
      typeof parsed.backgroundZoomWithVideo === "boolean"
        ? parsed.backgroundZoomWithVideo
        : DEFAULT_BACKGROUND_CONFIG.backgroundZoomWithVideo,
    volume: finiteOr(parsed.volume, DEFAULT_BACKGROUND_CONFIG.volume ?? 1),
    canvasWidth: optionalFinite(parsed.canvasWidth),
    canvasHeight: optionalFinite(parsed.canvasHeight),
    autoCanvasSourceId:
      typeof parsed.autoCanvasSourceId === "string"
        ? parsed.autoCanvasSourceId
        : null,
    customBackground:
      typeof parsed.customBackground === "string"
        ? parsed.customBackground
        : undefined,
    cursorPack:
      typeof parsed.cursorPack === "string" ? parsed.cursorPack : undefined,
    cursorDefaultVariant:
      typeof parsed.cursorDefaultVariant === "string"
        ? parsed.cursorDefaultVariant
        : undefined,
    cursorTextVariant:
      typeof parsed.cursorTextVariant === "string"
        ? parsed.cursorTextVariant
        : undefined,
    cursorPointerVariant:
      typeof parsed.cursorPointerVariant === "string"
        ? parsed.cursorPointerVariant
        : undefined,
    cursorOpenHandVariant:
      typeof parsed.cursorOpenHandVariant === "string"
        ? parsed.cursorOpenHandVariant
        : undefined,
  };
}

export function cloneBackgroundConfig(
  backgroundConfig: BackgroundConfig,
): BackgroundConfig {
  return normalizeBackgroundConfig(backgroundConfig);
}

export function equalBackgroundConfig(
  left: BackgroundConfig,
  right: BackgroundConfig,
): boolean {
  const leftEntries = Object.entries(left) as Array<
    [keyof BackgroundConfig, BackgroundConfig[keyof BackgroundConfig]]
  >;
  const rightEntries = Object.entries(right) as Array<
    [keyof BackgroundConfig, BackgroundConfig[keyof BackgroundConfig]]
  >;
  if (leftEntries.length !== rightEntries.length) return false;
  return leftEntries.every(([key, value]) => Object.is(value, right[key]));
}
