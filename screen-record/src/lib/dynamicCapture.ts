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

function hasValidDims(p: MousePosition): boolean {
  return typeof p.captureWidth === 'number' && Number.isFinite(p.captureWidth) && p.captureWidth > 1 &&
    typeof p.captureHeight === 'number' && Number.isFinite(p.captureHeight) && p.captureHeight > 1;
}

export function sampleCaptureDimensionsAtTime(
  time: number,
  positions: MousePosition[],
  fallbackWidth: number,
  fallbackHeight: number
): { width: number; height: number } {
  if (positions.length === 0) {
    return { width: Math.max(1, fallbackWidth), height: Math.max(1, fallbackHeight) };
  }

  // Binary search for first position with timestamp >= time (O(log n))
  let lo = 0, hi = positions.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (positions[mid].timestamp < time) lo = mid + 1;
    else hi = mid;
  }

  // Walk outward from insertion point to find nearest positions with valid dims
  let prev: MousePosition | null = null;
  let next: MousePosition | null = null;

  // Check for exact match first
  if (lo < positions.length && Math.abs(positions[lo].timestamp - time) < 0.001 && hasValidDims(positions[lo])) {
    return getMouseCaptureDimensions(positions[lo], fallbackWidth, fallbackHeight);
  }
  if (lo > 0 && Math.abs(positions[lo - 1].timestamp - time) < 0.001 && hasValidDims(positions[lo - 1])) {
    return getMouseCaptureDimensions(positions[lo - 1], fallbackWidth, fallbackHeight);
  }

  // Walk backward for prev with valid dims
  for (let i = lo - 1; i >= 0; i--) {
    if (hasValidDims(positions[i])) { prev = positions[i]; break; }
  }
  // Walk forward for next with valid dims
  for (let i = lo; i < positions.length; i++) {
    if (hasValidDims(positions[i])) { next = positions[i]; break; }
  }

  if (!prev && !next) {
    return { width: Math.max(1, fallbackWidth), height: Math.max(1, fallbackHeight) };
  }
  if (!prev) return getMouseCaptureDimensions(next!, fallbackWidth, fallbackHeight);
  if (!next) return getMouseCaptureDimensions(prev, fallbackWidth, fallbackHeight);

  const dt = next.timestamp - prev.timestamp;
  if (dt <= 0.000001) return getMouseCaptureDimensions(prev, fallbackWidth, fallbackHeight);

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
