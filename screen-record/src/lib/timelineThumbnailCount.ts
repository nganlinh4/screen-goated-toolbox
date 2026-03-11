import type { VideoSegment } from "@/types/video";
import { getTotalTrimDuration } from "@/lib/trimSegments";

const MIN_TIMELINE_THUMBNAIL_COUNT = 6;
const BASE_TIMELINE_THUMBNAIL_COUNT_CAP = 10;
const ADAPTIVE_TIMELINE_THUMBNAIL_COUNT_CAP = 28;

function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}

function getTimelineThumbnailWidthBucket(renderedWidthPx: number): number {
  if (renderedWidthPx >= 5600) return 28;
  if (renderedWidthPx >= 4200) return 24;
  if (renderedWidthPx >= 3000) return 20;
  if (renderedWidthPx >= 2200) return 16;
  if (renderedWidthPx >= 1600) return 12;
  if (renderedWidthPx >= 1200) return 10;
  if (renderedWidthPx >= 900) return 8;
  return 6;
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
