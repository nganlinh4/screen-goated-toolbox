import type { VideoSegment } from "@/types/video";
import { getTotalTrimDuration } from "@/lib/trimSegments";
import { clamp } from "@/lib/mathUtils";

const MIN_TIMELINE_THUMBNAIL_COUNT = 6;
const BASE_TIMELINE_THUMBNAIL_COUNT_CAP = 10;
const ADAPTIVE_TIMELINE_THUMBNAIL_COUNT_CAP = 240;

// Target ~150px per thumbnail — keeps visual density constant across all zoom levels.
// Super-zoomed timelines can exceed 30,000px, so keep the cap high enough
// that thumbnail cells do not stretch into blurry blocks.
const TARGET_PX_PER_THUMBNAIL = 150;

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
