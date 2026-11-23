/**
 * Chromium leaves a canvas blank when either side exceeds 32767 device pixels,
 * and a zoomed timeline lane can be wider than that at high DPR. Canvases cap
 * their backing store and let CSS stretch it, trading sharpness for content.
 */
export const MAX_CANVAS_SIDE_PX = 32767;

export interface TimelineCanvasSize {
  /** Backing-store size in device pixels, never above MAX_CANVAS_SIDE_PX. */
  width: number;
  height: number;
  /** Per-axis scale from CSS pixels to backing-store pixels. */
  scaleX: number;
  scaleY: number;
}

export function getTimelineCanvasSize(
  cssWidth: number,
  cssHeight: number,
  devicePixelRatio: number,
): TimelineCanvasSize {
  const dpr = devicePixelRatio > 0 ? devicePixelRatio : 1;
  const width = Math.max(1, Math.min(MAX_CANVAS_SIDE_PX, Math.round(cssWidth * dpr)));
  const height = Math.max(1, Math.min(MAX_CANVAS_SIDE_PX, Math.round(cssHeight * dpr)));
  return {
    width,
    height,
    scaleX: cssWidth > 0 ? width / cssWidth : dpr,
    scaleY: cssHeight > 0 ? height / cssHeight : dpr,
  };
}
