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
    if (targetCount === baseCount && (lastIssuedCount === null || lastIssuedCount === baseCount)) {
      return;
    }
    if (lastIssuedCount === targetCount) return;

    const timer = window.setTimeout(() => {
      lastIssuedCountRef.current = targetCount;
      void generateThumbnailsForSource({
        videoUrl: currentVideo,
        segment,
        thumbnailCount: targetCount,
      });
    }, TIMELINE_THUMBNAIL_DEBOUNCE_MS);

    return () => window.clearTimeout(timer);
  }, [
    baseCount,
    currentVideo,
    generateThumbnailsForSource,
    isPlaying,
    segment,
    targetCount,
    timelineCanvasWidthPx,
  ]);
}
