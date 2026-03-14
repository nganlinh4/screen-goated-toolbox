import type { CursorVisibilitySegment } from "@/types/video";
import {
  clampVisibilitySegmentsToDuration,
  getCursorVisibility,
  mergePointerSegments,
} from "@/lib/cursorHiding";

export function buildFullWebcamVisibilitySegments(
  duration: number,
): CursorVisibilitySegment[] {
  const safeDuration = Math.max(duration, 0);
  if (safeDuration <= 0) return [];
  return [
    {
      id: crypto.randomUUID(),
      startTime: 0,
      endTime: safeDuration,
    },
  ];
}

export function normalizeWebcamVisibilitySegments(
  segments: CursorVisibilitySegment[] | undefined | null,
  duration: number,
  fallbackVisible: boolean,
): CursorVisibilitySegment[] {
  const safeDuration = Math.max(duration, 0);
  const normalized = clampVisibilitySegmentsToDuration(
    mergePointerSegments((segments ?? []).map((segment) => ({ ...segment }))),
    safeDuration,
  );
  if (normalized.length > 0 || !fallbackVisible) {
    return normalized;
  }
  return buildFullWebcamVisibilitySegments(safeDuration);
}

export function isWebcamVisibleAtTime(
  segments: CursorVisibilitySegment[] | undefined | null,
  time: number,
): boolean {
  if (!segments || segments.length === 0) {
    return true;
  }
  return segments.some(
    (segment) => time >= segment.startTime && time <= segment.endTime,
  );
}

export function getWebcamVisibility(
  time: number,
  segments: CursorVisibilitySegment[] | undefined | null,
): { opacity: number; scale: number } {
  const visibility = getCursorVisibility(time, segments ?? undefined);
  return {
    opacity: visibility.opacity,
    // Keep webcam motion subtle: fade is primary, scale is only a slight assist.
    scale: 0.94 + (0.06 * visibility.opacity),
  };
}
