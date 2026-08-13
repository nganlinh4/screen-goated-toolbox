import type { CropRect } from "@/types/video";
import {
  CROP_ASPECT_RATIO_PRESETS,
  type AspectRatioPresetId,
} from "@/lib/aspectRatioPresets";
import { resolveCodecAlignedCropGeometry } from "@/lib/videoGeometry";

const DEFAULT_CROP: CropRect = { x: 0, y: 0, width: 1, height: 1 };
const RATIO_MATCH_TOLERANCE = 0.005;

export type CropResizeHandle = "nw" | "n" | "ne" | "w" | "e" | "sw" | "s" | "se";

export function getAspectRatioCrop(
  sourceWidth: number,
  sourceHeight: number,
  ratioWidth: number,
  ratioHeight: number,
  currentCrop: CropRect = DEFAULT_CROP,
): CropRect {
  if (
    sourceWidth <= 0 ||
    sourceHeight <= 0 ||
    ratioWidth <= 0 ||
    ratioHeight <= 0
  ) {
    return currentCrop;
  }

  const sourceRatio = sourceWidth / sourceHeight;
  const targetRatio = ratioWidth / ratioHeight;
  const width = targetRatio >= sourceRatio ? 1 : targetRatio / sourceRatio;
  const height = targetRatio >= sourceRatio ? sourceRatio / targetRatio : 1;
  const centerX = currentCrop.x + currentCrop.width / 2;
  const centerY = currentCrop.y + currentCrop.height / 2;
  const x = clamp(centerX - width / 2, 0, 1 - width);
  const y = clamp(centerY - height / 2, 0, 1 - height);

  return resolveCodecAlignedCropGeometry(sourceWidth, sourceHeight, {
    x,
    y,
    width,
    height,
  }).crop;
}

export function getActiveCropAspectRatioId(
  sourceWidth: number,
  sourceHeight: number,
  crop: CropRect,
): AspectRatioPresetId | null {
  if (sourceWidth <= 0 || sourceHeight <= 0) return null;
  const geometry = resolveCodecAlignedCropGeometry(sourceWidth, sourceHeight, crop);
  const currentRatio = geometry.width / geometry.height;
  const preset = CROP_ASPECT_RATIO_PRESETS.find(({ width, height }) => {
    const targetRatio = width / height;
    return Math.abs(currentRatio / targetRatio - 1) <= RATIO_MATCH_TOLERANCE;
  });
  return preset?.id ?? null;
}

export function isSourceCrop(
  sourceWidth: number,
  sourceHeight: number,
  crop: CropRect,
): boolean {
  if (sourceWidth <= 0 || sourceHeight <= 0) return false;
  const geometry = resolveCodecAlignedCropGeometry(sourceWidth, sourceHeight, crop);
  const sourceGeometry = resolveCodecAlignedCropGeometry(
    sourceWidth,
    sourceHeight,
    DEFAULT_CROP,
  );
  return geometry.width === sourceGeometry.width && geometry.height === sourceGeometry.height;
}

export function resizeCropWithAspectRatio(
  sourceWidth: number,
  sourceHeight: number,
  crop: CropRect,
  handle: CropResizeHandle,
  deltaX: number,
  deltaY: number,
  ratioWidth: number,
  ratioHeight: number,
): CropRect {
  if (
    sourceWidth <= 0 ||
    sourceHeight <= 0 ||
    ratioWidth <= 0 ||
    ratioHeight <= 0
  ) {
    return crop;
  }

  const normalizedRatio = (ratioWidth / ratioHeight) / (sourceWidth / sourceHeight);
  const hasWest = handle.includes("w");
  const hasEast = handle.includes("e");
  const hasNorth = handle.includes("n");
  const hasSouth = handle.includes("s");
  const centerX = crop.x + crop.width / 2;
  const centerY = crop.y + crop.height / 2;
  const right = crop.x + crop.width;
  const bottom = crop.y + crop.height;

  let desiredHeight: number;
  if ((hasWest || hasEast) && (hasNorth || hasSouth)) {
    const desiredWidth = hasWest ? crop.width - deltaX : crop.width + deltaX;
    const pointerHeight = hasNorth ? crop.height - deltaY : crop.height + deltaY;
    desiredHeight = (
      desiredWidth * normalizedRatio + pointerHeight
    ) / (normalizedRatio * normalizedRatio + 1);
  } else if (hasWest || hasEast) {
    const desiredWidth = hasWest ? crop.width - deltaX : crop.width + deltaX;
    desiredHeight = desiredWidth / normalizedRatio;
  } else {
    desiredHeight = hasNorth ? crop.height - deltaY : crop.height + deltaY;
  }

  const maxWidth = hasWest
    ? right
    : hasEast
      ? 1 - crop.x
      : 2 * Math.min(centerX, 1 - centerX);
  const maxHeight = hasNorth
    ? bottom
    : hasSouth
      ? 1 - crop.y
      : 2 * Math.min(centerY, 1 - centerY);
  const maximumHeight = Math.min(maxHeight, maxWidth / normalizedRatio);
  const minimumHeight = Math.min(
    maximumHeight,
    Math.max(0.05, 0.05 / normalizedRatio),
  );
  const height = clamp(desiredHeight, minimumHeight, maximumHeight);
  const width = height * normalizedRatio;
  const x = hasWest ? right - width : hasEast ? crop.x : centerX - width / 2;
  const y = hasNorth ? bottom - height : hasSouth ? crop.y : centerY - height / 2;

  return resolveCodecAlignedCropGeometry(sourceWidth, sourceHeight, {
    x,
    y,
    width,
    height,
  }).crop;
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}
