import { getBoundedCanvasSize } from "@/lib/canvasRenderBudget";

const INTERACTIVE_MAX_DIMENSION = 960;
const QUALITY_MAX_DIMENSION = 4096;
const QUALITY_MAX_PIXELS = 3840 * 2160;
const INTERACTIVE_SIZE_QUANTUM = 32;
export const MAX_CACHED_BACKGROUND_PIXELS = 24_000_000;
export const MAX_CACHED_BACKGROUNDS = 12;

function quantizeSize(value: number): number {
  return Math.max(INTERACTIVE_SIZE_QUANTUM, Math.round(value / INTERACTIVE_SIZE_QUANTUM) * INTERACTIVE_SIZE_QUANTUM);
}

export function getBuiltInBackgroundRenderSize(
  width: number,
  height: number,
  interactive: boolean
): { width: number; height: number } {
  const bounded = getBoundedCanvasSize(
    width,
    height,
    interactive ? INTERACTIVE_MAX_DIMENSION ** 2 : QUALITY_MAX_PIXELS,
    interactive ? INTERACTIVE_MAX_DIMENSION : QUALITY_MAX_DIMENSION,
  );
  if (!interactive) return { width: bounded.width, height: bounded.height };
  const scaledWidth = bounded.width;
  const scaledHeight = bounded.height;

  if (scaledWidth >= scaledHeight) {
    const quantizedWidth = quantizeSize(scaledWidth);
    return {
      width: quantizedWidth,
      height: Math.max(1, Math.round((scaledHeight / scaledWidth) * quantizedWidth)),
    };
  }

  const quantizedHeight = quantizeSize(scaledHeight);
  return {
    width: Math.max(1, Math.round((scaledWidth / scaledHeight) * quantizedHeight)),
    height: quantizedHeight,
  };
}

export function setCachedBuiltInBackground(
  cache: Map<string, HTMLCanvasElement>,
  key: string,
  canvas: HTMLCanvasElement
): void {
  if (cache.has(key)) cache.delete(key);
  cache.set(key, canvas);
  let cachedPixels = [...cache.values()].reduce(
    (total, entry) => total + entry.width * entry.height,
    0,
  );
  while (
    cache.size > MAX_CACHED_BACKGROUNDS ||
    cachedPixels > MAX_CACHED_BACKGROUND_PIXELS
  ) {
    const oldestKey = cache.keys().next().value;
    if (!oldestKey) break;
    const oldest = cache.get(oldestKey);
    if (oldest) cachedPixels -= oldest.width * oldest.height;
    cache.delete(oldestKey);
  }
}
