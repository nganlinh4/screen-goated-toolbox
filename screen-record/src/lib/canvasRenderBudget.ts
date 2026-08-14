export const MAX_REALTIME_TEMP_PIXELS = 3840 * 2160;
export const MAX_QUALITY_TEMP_PIXELS = 4096 * 4096;
export const MAX_OUTPUT_CANVAS_PIXELS = 7680 * 4320;
export const MAX_OUTPUT_CANVAS_DIMENSION = 8192;

export function getBoundedCanvasScale(
  width: number,
  height: number,
  requestedScale: number,
  maxPixels: number,
  maxDimension: number,
): number {
  const safeWidth = Number.isFinite(width) ? Math.max(1, width) : 1;
  const safeHeight = Number.isFinite(height) ? Math.max(1, height) : 1;
  const minimumScale = 1 / Math.max(safeWidth, safeHeight);
  const safeRequested = Number.isFinite(requestedScale)
    ? Math.max(minimumScale, requestedScale)
    : 1;
  const areaScale = Math.sqrt(
    Math.max(1, maxPixels) / (safeWidth * safeHeight),
  );
  const dimensionScale = Math.min(
    Math.max(1, maxDimension) / safeWidth,
    Math.max(1, maxDimension) / safeHeight,
  );
  return Math.max(
    minimumScale,
    Math.min(safeRequested, areaScale, dimensionScale),
  );
}

export function getBoundedCanvasSize(
  width: number,
  height: number,
  maxPixels: number,
  maxDimension: number,
): { width: number; height: number; scale: number } {
  const scale = getBoundedCanvasScale(
    width,
    height,
    1,
    maxPixels,
    maxDimension,
  );
  return {
    width: Math.max(
      1,
      Math.round((Number.isFinite(width) ? Math.max(1, width) : 1) * scale),
    ),
    height: Math.max(
      1,
      Math.round((Number.isFinite(height) ? Math.max(1, height) : 1) * scale),
    ),
    scale,
  };
}

/** Canonical even-sized output surface shared by preview and export. */
export function resolveOutputCanvasDimensions(
  width: number,
  height: number,
): { width: number; height: number } {
  const bounded = getBoundedCanvasSize(
    width,
    height,
    MAX_OUTPUT_CANVAS_PIXELS,
    MAX_OUTPUT_CANVAS_DIMENSION,
  );
  return {
    width: Math.max(2, Math.floor(bounded.width / 2) * 2),
    height: Math.max(2, Math.floor(bounded.height / 2) * 2),
  };
}
