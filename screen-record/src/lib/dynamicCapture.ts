import { CropRect, MousePosition } from '@/types/video';

function sanitizeDimension(value: number | undefined, fallback: number): number {
  if (typeof value !== 'number' || !Number.isFinite(value) || value <= 1) {
    return Math.max(1, fallback);
  }
  return Math.max(1, value);
}

export function getMouseCaptureDimensions(
  position: MousePosition | null | undefined,
  fallbackWidth: number,
  fallbackHeight: number
): { width: number; height: number } {
  return {
    width: sanitizeDimension(position?.captureWidth, fallbackWidth),
    height: sanitizeDimension(position?.captureHeight, fallbackHeight),
  };
}

export function normalizeMousePositionToVideoSpace(
  position: MousePosition,
  fallbackWidth: number,
  fallbackHeight: number
): MousePosition {
  const dims = getMouseCaptureDimensions(position, fallbackWidth, fallbackHeight);
  const targetWidth = Math.max(1, fallbackWidth || dims.width);
  const targetHeight = Math.max(1, fallbackHeight || dims.height);

  if (
    Math.abs(dims.width - targetWidth) < 0.001 &&
    Math.abs(dims.height - targetHeight) < 0.001
  ) {
    return position;
  }

  return {
    ...position,
    x: (position.x / dims.width) * targetWidth,
    y: (position.y / dims.height) * targetHeight,
  };
}

export function normalizeMousePositionsToVideoSpace(
  positions: MousePosition[],
  fallbackWidth: number,
  fallbackHeight: number
): MousePosition[] {
  if (positions.length === 0) return positions;

  let changed = false;
  const normalized = positions.map((position) => {
    const next = normalizeMousePositionToVideoSpace(position, fallbackWidth, fallbackHeight);
    if (next !== position) changed = true;
    return next;
  });

  return changed ? normalized : positions;
}

export function sampleCaptureDimensionsAtTime(
  time: number,
  positions: MousePosition[],
  fallbackWidth: number,
  fallbackHeight: number
): { width: number; height: number } {
  const withDims = positions.filter((position) =>
    typeof position.captureWidth === 'number' &&
    Number.isFinite(position.captureWidth) &&
    position.captureWidth > 1 &&
    typeof position.captureHeight === 'number' &&
    Number.isFinite(position.captureHeight) &&
    position.captureHeight > 1
  );

  if (withDims.length === 0) {
    return {
      width: Math.max(1, fallbackWidth),
      height: Math.max(1, fallbackHeight),
    };
  }

  const exact = withDims.find((position) => Math.abs(position.timestamp - time) < 0.001);
  if (exact) {
    return getMouseCaptureDimensions(exact, fallbackWidth, fallbackHeight);
  }

  const nextIndex = withDims.findIndex((position) => position.timestamp > time);
  if (nextIndex <= 0) {
    return getMouseCaptureDimensions(withDims[0], fallbackWidth, fallbackHeight);
  }
  if (nextIndex === -1) {
    return getMouseCaptureDimensions(withDims[withDims.length - 1], fallbackWidth, fallbackHeight);
  }

  const prev = withDims[nextIndex - 1];
  const next = withDims[nextIndex];
  const dt = next.timestamp - prev.timestamp;
  if (dt <= 0.000001) {
    return getMouseCaptureDimensions(prev, fallbackWidth, fallbackHeight);
  }

  const t = (time - prev.timestamp) / dt;
  const prevDims = getMouseCaptureDimensions(prev, fallbackWidth, fallbackHeight);
  const nextDims = getMouseCaptureDimensions(next, fallbackWidth, fallbackHeight);
  return {
    width: prevDims.width + (nextDims.width - prevDims.width) * t,
    height: prevDims.height + (nextDims.height - prevDims.height) * t,
  };
}

export function getContainedRect(
  containerWidth: number,
  containerHeight: number,
  contentWidth: number,
  contentHeight: number,
  scale = 1
): { width: number; height: number; left: number; top: number } {
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

  const width = fitWidth * scale;
  const height = fitHeight * scale;
  return {
    width,
    height,
    left: (safeContainerW - width) / 2,
    top: (safeContainerH - height) / 2,
  };
}

export function getLogicalCropSize(
  captureWidth: number,
  captureHeight: number,
  crop: CropRect | undefined,
  legacyCropBottomPercent = 0
): { width: number; height: number } {
  const safeCrop = crop || { x: 0, y: 0, width: 1, height: 1 };
  const legacyCropFactor = Math.max(0, Math.min(1, 1 - (legacyCropBottomPercent / 100)));
  return {
    width: Math.max(1, captureWidth * safeCrop.width),
    height: Math.max(1, captureHeight * safeCrop.height * legacyCropFactor),
  };
}
