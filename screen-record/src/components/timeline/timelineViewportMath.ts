import type { TimelineVisibleRange } from "./SegmentBlocksCanvas";

export const MIN_TIMELINE_ZOOM = 1;
export const MAX_TIMELINE_ZOOM = 12;
export const WHEEL_ZOOM_SENSITIVITY = 0.002;
export const FOLLOW_SUPPRESS_MS = 900;
export const FOLLOW_SAFE_LEFT_RATIO = 0.2;
export const FOLLOW_SAFE_RIGHT_RATIO = 0.8;
export const FOLLOW_TARGET_RATIO = 0.6;
export const VIEWPORT_BLEED_PX = 16;
export const MIN_SCROLLBAR_THUMB_PX = 36;

export interface TimelineWheelAnchor {
  anchorTime: number;
  pointerContentX: number;
}

export function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}

export function getVisibleContentWidth(viewport: HTMLDivElement): number {
  return Math.max(viewport.clientWidth - VIEWPORT_BLEED_PX * 2, 1);
}

export function getContentWidth(
  viewport: HTMLDivElement,
  timeline: HTMLDivElement | null,
  zoom: number,
): number {
  const measuredWidth = timeline?.getBoundingClientRect().width ?? 0;
  if (measuredWidth > 0) return measuredWidth;
  return getVisibleContentWidth(viewport) * zoom;
}

export function getMaxScroll(
  viewport: HTMLDivElement,
  contentWidth: number,
): number {
  return Math.max(0, contentWidth - getVisibleContentWidth(viewport));
}

export function buildVisibleTimeRange(
  scrollLeft: number,
  visibleWidth: number,
  contentWidth: number,
  duration: number,
): TimelineVisibleRange {
  const visibleDuration = duration * (visibleWidth / Math.max(contentWidth, 1));
  const buffer = Math.max(1, visibleDuration * 0.65);
  return {
    startTime: clamp(
      (scrollLeft / Math.max(contentWidth, 1)) * duration - buffer,
      0,
      duration,
    ),
    endTime: clamp(
      ((scrollLeft + visibleWidth) / Math.max(contentWidth, 1)) * duration +
        buffer,
      0,
      duration,
    ),
  };
}
