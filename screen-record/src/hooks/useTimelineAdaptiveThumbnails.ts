import { useEffect, useMemo, useRef } from "react";
import type { VideoSegment } from "@/types/video";
import {
  getAdaptiveTimelineThumbnailCount,
  getBaseTimelineThumbnailCount,
} from "@/lib/timelineThumbnailCount";

const TIMELINE_THUMBNAIL_DEBOUNCE_MS = 220;

interface UseTimelineAdaptiveThumbnailsOptions {
  timelineCanvasWidthPx: number;
  segment: VideoSegment | null;
  currentVideo: string | null;
  currentRawVideoPath?: string | null;
  thumbnailsLength: number;
  isPlaying: boolean;
  generateThumbnailsForSource: (options?: {
    videoUrl?: string | null;
    filePath?: string;
    segment?: VideoSegment | null;
    deferMs?: number;
    thumbnailCount?: number;
  }) => Promise<void>;
}

export function useTimelineAdaptiveThumbnails({
  timelineCanvasWidthPx,
  segment,
  currentVideo,
  currentRawVideoPath,
  thumbnailsLength,
  isPlaying,
  generateThumbnailsForSource,
}: UseTimelineAdaptiveThumbnailsOptions) {
  const lastIssuedCountRef = useRef<number | null>(null);
  const baseCount = useMemo(
    () => getBaseTimelineThumbnailCount(segment),
    [segment],
  );
  const targetCount = useMemo(
    () =>
      getAdaptiveTimelineThumbnailCount(segment, timelineCanvasWidthPx),
    [segment, timelineCanvasWidthPx],
  );

  useEffect(() => {
    lastIssuedCountRef.current = null;
  }, [currentVideo, segment]);

  useEffect(() => {
    if (!segment || !currentVideo || isPlaying || timelineCanvasWidthPx <= 0) {
      return;
    }

    const lastIssuedCount = lastIssuedCountRef.current;
    const needsInitialStrip = thumbnailsLength === 0 && lastIssuedCount === null;
    if (
      !needsInitialStrip &&
      targetCount === baseCount &&
      (lastIssuedCount === null || lastIssuedCount === baseCount)
    ) {
      return;
    }
    if (!needsInitialStrip && lastIssuedCount === targetCount) return;
    const thumbnailCount = needsInitialStrip ? baseCount : targetCount;

    const timer = window.setTimeout(() => {
      lastIssuedCountRef.current = thumbnailCount;
      void generateThumbnailsForSource({
        videoUrl: currentVideo,
        filePath: currentRawVideoPath?.trim() || undefined,
        segment,
        thumbnailCount,
      });
    }, TIMELINE_THUMBNAIL_DEBOUNCE_MS);

    return () => window.clearTimeout(timer);
  }, [
    baseCount,
    currentVideo,
    currentRawVideoPath,
    generateThumbnailsForSource,
    isPlaying,
    segment,
    targetCount,
    thumbnailsLength,
    timelineCanvasWidthPx,
  ]);
}
