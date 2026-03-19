import type { CSSProperties } from "react";
import {
  BackgroundConfig,
  VideoSegment,
} from "@/types/video";
import { DEFAULT_BUILT_IN_BACKGROUND_ID } from "@/lib/backgroundPresets";
import type { ProjectsPreviewRectSnapshot } from "@/components/ProjectsView";

export const LAST_BG_CONFIG_KEY = "screen-record-last-background-config-v1";
export const PROJECT_SAVE_DEBUG = false;
export const PROJECT_SWITCH_DEBUG = false;
export const BACKGROUND_MUTATION_DEBUG = false;
export const PLAYBACK_RESET_DEBUG = false;

export const sv = (v: number, min: number, max: number): CSSProperties =>
  ({ "--value-pct": `${((v - min) / (max - min)) * 100}%` }) as CSSProperties;

export const POPULAR_CANVAS_RATIO_PRESETS = [
  { id: "landscape-16-9", label: "16:9", width: 16, height: 9 },
  { id: "portrait-9-16", label: "9:16", width: 9, height: 16 },
  { id: "square-1-1", label: "1:1", width: 1, height: 1 },
  { id: "portrait-4-5", label: "4:5", width: 4, height: 5 },
  { id: "cinema-21-9", label: "21:9", width: 21, height: 9 },
] as const;

export function roundToEven(value: number): number {
  const rounded = Math.max(2, Math.round(value));
  return rounded % 2 === 0 ? rounded : rounded + 1;
}

export function getCanvasRatioDimensions(
  baseWidth: number,
  baseHeight: number,
  ratioWidth: number,
  ratioHeight: number,
) {
  const safeBaseWidth = Math.max(2, baseWidth || 0);
  const safeBaseHeight = Math.max(2, baseHeight || 0);
  const area = safeBaseWidth * safeBaseHeight;
  const ratio = ratioWidth / ratioHeight;

  if (!Number.isFinite(area) || area <= 0 || !Number.isFinite(ratio) || ratio <= 0) {
    return {
      width: roundToEven(ratioWidth * 120),
      height: roundToEven(ratioHeight * 120),
    };
  }

  const width = Math.sqrt(area * ratio);
  const height = width / ratio;

  return {
    width: roundToEven(width),
    height: roundToEven(height),
  };
}

export function isCanvasRatioPresetActive(
  canvasWidth: number | undefined,
  canvasHeight: number | undefined,
  ratioWidth: number,
  ratioHeight: number,
) {
  if (!canvasWidth || !canvasHeight) return false;
  const currentRatio = canvasWidth / canvasHeight;
  const targetRatio = ratioWidth / ratioHeight;
  return Math.abs(currentRatio - targetRatio) <= 0.018;
}

export const DEFAULT_BACKGROUND_CONFIG: BackgroundConfig = {
  scale: 90,
  borderRadius: 32,
  backgroundType: DEFAULT_BUILT_IN_BACKGROUND_ID,
  shadow: 100,
  cursorScale: 5,
  cursorMovementDelay: 0,
  cursorShadow: 100,
  cursorWiggleStrength: 0.3,
  cursorTiltAngle: -10,
  motionBlurCursor: 25,
  motionBlurZoom: 10,
  motionBlurPan: 10,
  cursorPack: "macos26",
  cursorDefaultVariant: "macos26",
  cursorTextVariant: "macos26",
  cursorPointerVariant: "macos26",
  cursorOpenHandVariant: "macos26",
};

export function getInitialBackgroundConfig(): BackgroundConfig {
  try {
    const raw = localStorage.getItem(LAST_BG_CONFIG_KEY);
    if (!raw) return DEFAULT_BACKGROUND_CONFIG;
    const parsed = JSON.parse(raw) as Partial<BackgroundConfig>;
    return {
      ...DEFAULT_BACKGROUND_CONFIG,
      ...parsed,
    };
  } catch {
    return DEFAULT_BACKGROUND_CONFIG;
  }
}

export function summarizeBackgroundConfig(backgroundConfig: BackgroundConfig | null | undefined) {
  return backgroundConfig
    ? {
        backgroundType: backgroundConfig.backgroundType,
        canvasMode: backgroundConfig.canvasMode ?? "auto",
        canvasWidth: backgroundConfig.canvasWidth ?? null,
        canvasHeight: backgroundConfig.canvasHeight ?? null,
        autoCanvasSourceId: backgroundConfig.autoCanvasSourceId ?? null,
        scale: backgroundConfig.scale,
      }
    : null;
}

export function summarizeSegment(segment: VideoSegment | null | undefined) {
  return segment
    ? {
        trimStart: segment.trimStart,
        trimEnd: segment.trimEnd,
        crop: segment.crop ?? null,
      }
    : null;
}

export function toPreviewRectSnapshot(
  rect: DOMRect | null | undefined,
): ProjectsPreviewRectSnapshot | null {
  if (!rect || rect.width <= 0 || rect.height <= 0) return null;
  return {
    left: Number(rect.left.toFixed(2)),
    top: Number(rect.top.toFixed(2)),
    width: Number(rect.width.toFixed(2)),
    height: Number(rect.height.toFixed(2)),
  };
}
