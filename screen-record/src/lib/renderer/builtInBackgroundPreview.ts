const INTERACTIVE_MAX_DIMENSION = 960;
const INTERACTIVE_SIZE_QUANTUM = 32;
const MAX_CACHED_BACKGROUNDS = 48;

function quantizeSize(value: number): number {
  return Math.max(INTERACTIVE_SIZE_QUANTUM, Math.round(value / INTERACTIVE_SIZE_QUANTUM) * INTERACTIVE_SIZE_QUANTUM);
}

export function getBuiltInBackgroundRenderSize(
  width: number,
  height: number,
  interactive: boolean
): { width: number; height: number } {
  if (!interactive) return { width, height };

  const maxSide = Math.max(width, height);
  const scale = maxSide > INTERACTIVE_MAX_DIMENSION ? INTERACTIVE_MAX_DIMENSION / maxSide : 1;
  const scaledWidth = Math.max(1, Math.round(width * scale));
  const scaledHeight = Math.max(1, Math.round(height * scale));

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
  cache.set(key, canvas);
  while (cache.size > MAX_CACHED_BACKGROUNDS) {
    const oldestKey = cache.keys().next().value;
    if (!oldestKey) break;
    cache.delete(oldestKey);
  }
}
