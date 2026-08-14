const DEFAULT_FRAME_RATE = 60;
const MIN_FRAME_RATE = 1;
const MAX_FRAME_RATE = 240;

type FrameRateListener = () => void;

let currentFrameRate = DEFAULT_FRAME_RATE;
const listeners = new Set<FrameRateListener>();

function normalizeFrameRate(value: number): number {
  if (!Number.isFinite(value)) return DEFAULT_FRAME_RATE;
  return Math.max(MIN_FRAME_RATE, Math.min(MAX_FRAME_RATE, Math.round(value)));
}

export function getPreviewExportFrameRate(): number {
  return currentFrameRate;
}

export function setPreviewExportFrameRate(value: number): void {
  const nextFrameRate = normalizeFrameRate(value);
  if (nextFrameRate === currentFrameRate) return;
  currentFrameRate = nextFrameRate;
  for (const listener of listeners) listener();
}

export function subscribePreviewExportFrameRate(
  listener: FrameRateListener,
): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}
