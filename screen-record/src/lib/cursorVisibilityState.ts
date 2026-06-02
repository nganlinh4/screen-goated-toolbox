import { CursorVisibilitySegment } from "@/types/video";

const FADE_IN_DURATION = 0.2;
const FADE_OUT_DURATION = 0.25;
const MIN_FULLY_VISIBLE_DURATION = 0.06;
const SCALE_HIDDEN = 0.5;
const SCALE_VISIBLE = 1.0;

function easeOutCubic(t: number): number {
  return 1 - Math.pow(1 - t, 3);
}

function easeInCubic(t: number): number {
  return t * t * t;
}

function getSegmentFadeDurations(
  startTime: number,
  endTime: number,
): { fadeIn: number; fadeOut: number } {
  const duration = Math.max(0, endTime - startTime);
  const preferredTotal = FADE_IN_DURATION + FADE_OUT_DURATION;
  const maxFadeTotal = Math.max(0, duration - MIN_FULLY_VISIBLE_DURATION);

  if (duration <= 0 || maxFadeTotal <= 0 || preferredTotal <= 0) {
    return { fadeIn: 0, fadeOut: 0 };
  }

  const actualTotal = Math.min(preferredTotal, maxFadeTotal);
  const fadeIn = actualTotal * (FADE_IN_DURATION / preferredTotal);
  const fadeOut = actualTotal - fadeIn;
  return { fadeIn, fadeOut };
}

/**
 * Pure, deterministic function to compute cursor visibility at a given time.
 * Used identically for preview AND export baking (WYSIWYG).
 */
export function getCursorVisibility(
  time: number,
  segments: CursorVisibilitySegment[] | undefined,
): { opacity: number; scale: number } {
  if (!segments) {
    return { opacity: 1.0, scale: 1.0 };
  }

  if (segments.length === 0) {
    return { opacity: 0.0, scale: SCALE_HIDDEN };
  }

  let lo = 0;
  let hi = segments.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (segments[mid].startTime <= time) lo = mid + 1;
    else hi = mid;
  }
  const idx = lo - 1;

  if (idx < 0 || time > segments[idx].endTime) {
    return { opacity: 0.0, scale: SCALE_HIDDEN };
  }

  const seg = segments[idx];
  const { fadeIn, fadeOut } = getSegmentFadeDurations(
    seg.startTime,
    seg.endTime,
  );
  const fadeInEnd = seg.startTime + fadeIn;
  const fadeOutStart = seg.endTime - fadeOut;

  if (fadeIn > 0 && time < fadeInEnd) {
    const t = (time - seg.startTime) / fadeIn;
    const eased = easeOutCubic(Math.max(0, Math.min(1, t)));
    return {
      opacity: eased,
      scale: SCALE_HIDDEN + (SCALE_VISIBLE - SCALE_HIDDEN) * eased,
    };
  }

  if (fadeOut > 0 && time > fadeOutStart) {
    const t = (time - fadeOutStart) / fadeOut;
    const eased = 1 - easeInCubic(Math.max(0, Math.min(1, t)));
    return {
      opacity: eased,
      scale: SCALE_HIDDEN + (SCALE_VISIBLE - SCALE_HIDDEN) * eased,
    };
  }

  return { opacity: 1.0, scale: 1.0 };
}
