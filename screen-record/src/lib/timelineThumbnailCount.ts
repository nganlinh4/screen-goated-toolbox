import type { VideoSegment } from "@/types/video";
import { getTotalTrimDuration } from "@/lib/trimSegments";

const MIN_TIMELINE_THUMBNAIL_COUNT = 6;
const BASE_TIMELINE_THUMBNAIL_COUNT_CAP = 10;
const ADAPTIVE_TIMELINE_THUMBNAIL_COUNT_CAP = 100;

// Target ~150px per thumbnail — keeps visual density constant across all zoom levels.
// Timeline zoom goes up to 12x, so a 1400px viewport → 16,800px canvas at max zoom.
// At 150px/thumb that's 112 (capped at 100), giving ~9 visible thumbnails per viewport.
const TARGET_PX_PER_THUMBNAIL = 150;

function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}

function getTimelineThumbnailWidthBucket(renderedWidthPx: number): number {
  return Math.ceil(renderedWidthPx / TARGET_PX_PER_THUMBNAIL);
}

export function getBaseTimelineThumbnailCount(
  thumbnailSegment: VideoSegment | null | undefined,
): number {
  if (!thumbnailSegment) return MIN_TIMELINE_THUMBNAIL_COUNT;
  const trimDuration = Math.max(
    0,
    getTotalTrimDuration(
      thumbnailSegment,
      Math.max(thumbnailSegment.trimEnd, 0.001),
    ),
  );
  return clamp(
    Math.ceil(trimDuration / 3),
    MIN_TIMELINE_THUMBNAIL_COUNT,
    BASE_TIMELINE_THUMBNAIL_COUNT_CAP,
  );
}

export function getAdaptiveTimelineThumbnailCount(
  thumbnailSegment: VideoSegment | null | undefined,
  renderedTimelineWidthPx: number,
): number {
  const baseCount = getBaseTimelineThumbnailCount(thumbnailSegment);
  const widthBucketCount = getTimelineThumbnailWidthBucket(
    Math.max(renderedTimelineWidthPx, 0),
  );
  return clamp(
    Math.max(baseCount, widthBucketCount),
    baseCount,
    ADAPTIVE_TIMELINE_THUMBNAIL_COUNT_CAP,
  );
}
