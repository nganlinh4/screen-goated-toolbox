// React hook for canvas-based timeline rendering.
//
// Manages: canvas ref, DPR scaling, RAF draw loop, thumbnail ImageBitmap cache.
// Calls drawTimeline() on every state change via requestAnimationFrame.

import { useRef, useEffect, useCallback } from 'react';
import type { VideoSegment } from '@/types/video';
import type { TimelineRulerTick } from './timelineRuler';
import { drawTimeline, TOTAL_CANVAS_HEIGHT, type TimelineDrawState } from './TimelineCanvas';

interface UseTimelineCanvasOptions {
  segment: VideoSegment | null;
  duration: number;
  currentTime: number;
  zoom: number;
  canvasWidthPx: number;
  viewportRef: React.RefObject<HTMLDivElement>;
  rulerTicks: TimelineRulerTick[];
  thumbnails: string[];
  isDark: boolean;
  isDeviceAudioAvailable: boolean;
  isMicAudioAvailable: boolean;
  isWebcamAvailable: boolean;
}

export function useTimelineCanvas({
  segment,
  duration,
  currentTime,
  zoom,
  canvasWidthPx,
  viewportRef,
  rulerTicks,
  thumbnails,
  isDark,
  isDeviceAudioAvailable,
  isMicAudioAvailable,
  isWebcamAvailable,
}: UseTimelineCanvasOptions) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const rafRef = useRef<number | null>(null);
  const thumbnailImagesRef = useRef<ImageBitmap[]>([]);
  const thumbnailUrlsRef = useRef<string[]>([]);

  // Convert thumbnail data URLs to ImageBitmap (cached, async)
  useEffect(() => {
    if (
      thumbnails.length === thumbnailUrlsRef.current.length &&
      thumbnails.every((url, i) => url === thumbnailUrlsRef.current[i])
    ) {
      return;
    }
    thumbnailUrlsRef.current = thumbnails;

    let cancelled = false;
    const loadAll = async () => {
      const images: ImageBitmap[] = [];
      for (const url of thumbnails) {
        if (cancelled) return;
        try {
          const resp = await fetch(url);
          const blob = await resp.blob();
          const bmp = await createImageBitmap(blob);
          images.push(bmp);
        } catch {
          // Skip failed thumbnails
        }
      }
      if (!cancelled) {
        thumbnailImagesRef.current = images;
      }
    };
    void loadAll();
    return () => {
      cancelled = true;
    };
  }, [thumbnails]);

  // Schedule a draw on next animation frame
  const scheduleRedraw = useCallback(() => {
    if (rafRef.current !== null) return;
    rafRef.current = requestAnimationFrame(() => {
      rafRef.current = null;
      const canvas = canvasRef.current;
      const viewport = viewportRef.current;
      if (!canvas || !viewport) return;

      const dpr = window.devicePixelRatio || 1;
      const scrollLeft = viewport.scrollLeft;
      const viewportWidth = viewport.clientWidth;

      // Resize canvas if needed
      const cssWidth = canvasWidthPx;
      const cssHeight = TOTAL_CANVAS_HEIGHT;
      const pixelWidth = Math.round(cssWidth * dpr);
      const pixelHeight = Math.round(cssHeight * dpr);

      if (canvas.width !== pixelWidth || canvas.height !== pixelHeight) {
        canvas.width = pixelWidth;
        canvas.height = pixelHeight;
        canvas.style.width = `${cssWidth}px`;
        canvas.style.height = `${cssHeight}px`;
      }

      const ctx = canvas.getContext('2d');
      if (!ctx) return;

      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);

      const state: TimelineDrawState = {
        segment,
        duration,
        currentTime,
        zoom,
        scrollLeft,
        viewportWidth,
        canvasWidthPx: cssWidth,
        rulerTicks,
        thumbnails,
        thumbnailImages: thumbnailImagesRef.current,
        isDark,
        isDeviceAudioAvailable,
        isMicAudioAvailable,
        isWebcamAvailable,
        dpr,
      };

      drawTimeline(ctx, state);
    });
  }, [
    segment,
    duration,
    currentTime,
    zoom,
    canvasWidthPx,
    viewportRef,
    rulerTicks,
    thumbnails,
    isDark,
    isDeviceAudioAvailable,
    isMicAudioAvailable,
    isWebcamAvailable,
  ]);

  // Redraw on any state change
  useEffect(() => {
    scheduleRedraw();
  }, [scheduleRedraw]);

  // Redraw on scroll
  useEffect(() => {
    const viewport = viewportRef.current;
    if (!viewport) return;

    const onScroll = () => scheduleRedraw();
    viewport.addEventListener('scroll', onScroll, { passive: true });
    return () => viewport.removeEventListener('scroll', onScroll);
  }, [viewportRef, scheduleRedraw]);

  // Cleanup
  useEffect(() => {
    return () => {
      if (rafRef.current !== null) {
        cancelAnimationFrame(rafRef.current);
      }
    };
  }, []);

  return {
    canvasRef,
    canvasHeight: TOTAL_CANVAS_HEIGHT,
  };
}
