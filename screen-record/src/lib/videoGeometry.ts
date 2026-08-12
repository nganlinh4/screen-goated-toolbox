import type { BackgroundConfig, CropRect, VideoSegment } from '@/types/video';

export interface VideoPlacementRect {
  width: number;
  height: number;
  left: number;
  top: number;
}

export interface CodecAlignedCropGeometry {
  crop: CropRect;
  width: number;
  height: number;
}

export function resolveCodecAlignedCropGeometry(
  sourceWidth: number,
  sourceHeight: number,
  crop: CropRect | null | undefined,
): CodecAlignedCropGeometry {
  const width = Math.max(2, Math.round(sourceWidth));
  const height = Math.max(2, Math.round(sourceHeight));
  const requested = sanitizeCrop(crop);
  const cropWidth = nearestSupportedDimension(requested.width * width, width);
  const cropHeight = nearestSupportedDimension(requested.height * height, height);
  const centerX = (requested.x + requested.width / 2) * width;
  const centerY = (requested.y + requested.height / 2) * height;
  const cropX = clamp(Math.round(centerX - cropWidth / 2), 0, width - cropWidth);
  const cropY = clamp(Math.round(centerY - cropHeight / 2), 0, height - cropHeight);

  return {
    crop: {
      x: cropX / width,
      y: cropY / height,
      width: cropWidth / width,
      height: cropHeight / height,
    },
    width: cropWidth,
    height: cropHeight,
  };
}

export function normalizeAutoCanvasSegment(
  segment: VideoSegment,
  backgroundConfig: Pick<BackgroundConfig, 'canvasMode'> | null | undefined,
  sourceWidth: number,
  sourceHeight: number,
  isCanvasAuthority = true,
): VideoSegment {
  if (!isCanvasAuthority || backgroundConfig?.canvasMode === 'custom') {
    return segment;
  }
  return {
    ...segment,
    crop: resolveCodecAlignedCropGeometry(
      sourceWidth,
      sourceHeight,
      segment.crop,
    ).crop,
  };
}

export function getContainedRect(
  containerWidth: number,
  containerHeight: number,
  contentWidth: number,
  contentHeight: number,
  scale = 1,
): VideoPlacementRect {
  const safeContainerW = Math.max(1, containerWidth);
  const safeContainerH = Math.max(1, containerHeight);
  const safeContentW = Math.max(1, contentWidth);
  const safeContentH = Math.max(1, contentHeight);
  const contentAspect = safeContentW / safeContentH;
  const containerAspect = safeContainerW / safeContainerH;

  let fitWidth: number;
  let fitHeight: number;
  if (contentAspect > containerAspect) {
    fitWidth = safeContainerW;
    fitHeight = fitWidth / contentAspect;
  } else {
    fitHeight = safeContainerH;
    fitWidth = fitHeight * contentAspect;
  }

  return centerScaledRect(safeContainerW, safeContainerH, fitWidth, fitHeight, scale);
}

export function getVideoPlacementRect(
  containerWidth: number,
  containerHeight: number,
  contentWidth: number,
  contentHeight: number,
  scale: number,
): VideoPlacementRect {
  return getContainedRect(
    containerWidth,
    containerHeight,
    contentWidth,
    contentHeight,
    scale,
  );
}

export function coversCanvas(
  rect: VideoPlacementRect,
  canvasWidth: number,
  canvasHeight: number,
  epsilon = 0.001,
): boolean {
  return rect.left <= epsilon &&
    rect.top <= epsilon &&
    rect.left + rect.width >= canvasWidth - epsilon &&
    rect.top + rect.height >= canvasHeight - epsilon;
}

function centerScaledRect(
  containerWidth: number,
  containerHeight: number,
  baseWidth: number,
  baseHeight: number,
  scale: number,
): VideoPlacementRect {
  const width = baseWidth * scale;
  const height = baseHeight * scale;
  return {
    width,
    height,
    left: (containerWidth - width) / 2,
    top: (containerHeight - height) / 2,
  };
}

function nearestSupportedDimension(value: number, sourceDimension: number): number {
  const maxEven = Math.max(2, Math.floor(sourceDimension / 2) * 2);
  const nearestEven = Math.max(2, Math.round(value / 2) * 2);
  return Math.min(nearestEven, maxEven);
}

function sanitizeCrop(crop: CropRect | null | undefined): CropRect {
  const x = clamp(Number.isFinite(crop?.x) ? crop!.x : 0, 0, 1);
  const y = clamp(Number.isFinite(crop?.y) ? crop!.y : 0, 0, 1);
  const width = clamp(Number.isFinite(crop?.width) ? crop!.width : 1, 0, 1 - x);
  const height = clamp(Number.isFinite(crop?.height) ? crop!.height : 1, 0, 1 - y);
  return { x, y, width, height };
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}
