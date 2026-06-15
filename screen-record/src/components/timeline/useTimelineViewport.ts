import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type PointerEvent,
  type RefObject,
} from "react";
import type { VideoSegment } from "@/types/video";
import { clampToTrimSegments } from "@/lib/trimSegments";
import { SpringSolver } from "@/lib/spring";
import type { TimelineVisibleRange } from "./SegmentBlocksCanvas";
import { useTimelineScrollbarDrag } from "./useTimelineScrollbarDrag";
import { useTimelineWheelZoom } from "./useTimelineWheelZoom";
import {
  buildVisibleTimeRange,
  clamp,
  FOLLOW_SAFE_LEFT_RATIO,
  FOLLOW_SAFE_RIGHT_RATIO,
  FOLLOW_SUPPRESS_MS,
  FOLLOW_TARGET_RATIO,
  getContentWidth,
  getMaxScroll,
  getVisibleContentWidth,
  MIN_SCROLLBAR_THUMB_PX,
  MIN_TIMELINE_ZOOM,
  type TimelineWheelAnchor,
} from "./timelineViewportMath";

interface UseTimelineViewportOptions {
  duration: number;
  currentTime: number;
  segment: VideoSegment | null;
  timelineRef: RefObject<HTMLDivElement | null>;
  videoRef: RefObject<HTMLVideoElement | null>;
  isPlaying: boolean;
  isInteracting: boolean;
  disableVideoSync?: boolean;
}

interface UseTimelineViewportResult {
  viewportRef: RefObject<HTMLDivElement | null>;
  scrollbarTrackRef: RefObject<HTMLDivElement | null>;
  scrollbarThumbRef: RefObject<HTMLDivElement | null>;
  zoom: number;
  showScrollbar: boolean;
  canvasWidth: string;
  canvasWidthPx: number;
  visibleTimeRange: TimelineVisibleRange | null;
  handleScrollbarTrackPointerDown: (
    event: PointerEvent<HTMLDivElement>,
  ) => void;
  handleScrollbarThumbPointerDown: (
    event: PointerEvent<HTMLDivElement>,
  ) => void;
}

export function useTimelineViewport({
  duration,
  currentTime,
  segment,
  timelineRef,
  videoRef,
  isPlaying,
  isInteracting,
  disableVideoSync = false,
}: UseTimelineViewportOptions): UseTimelineViewportResult {
  const viewportRef = useRef<HTMLDivElement>(null);
  const scrollbarTrackRef = useRef<HTMLDivElement>(null);
  const scrollbarThumbRef = useRef<HTMLDivElement>(null);
  const [zoom, setZoom] = useState(1);
  const [viewportWidth, setViewportWidth] = useState(0);
  const [visibleTimeRange, setVisibleTimeRange] = useState<TimelineVisibleRange | null>(null);
  const [showScrollbarState, setShowScrollbarState] = useState(false);
  const pendingScrollbarRafRef = useRef<number | null>(null);
  const pendingVisibleRangeRafRef = useRef<number | null>(null);
  const animationFrameRef = useRef<number | null>(null);
  const releaseProgrammaticScrollRef = useRef<number | null>(null);
  const lastFrameTimeRef = useRef<number | null>(null);
  const pendingWheelAnchorRef = useRef<TimelineWheelAnchor | null>(null);
  const durationRef = useRef(duration);
  const currentTimeRef = useRef(currentTime);
  const segmentRef = useRef(segment);
  const isPlayingRef = useRef(isPlaying);
  const isInteractingRef = useRef(isInteracting);
  const disableVideoSyncRef = useRef(disableVideoSync);
  const zoomRef = useRef(zoom);
  const showScrollbarRef = useRef(showScrollbarState);
  const followSuppressedUntilRef = useRef(0);
  const programmaticScrollRef = useRef(false);
  const followActiveRef = useRef(false);
  const followSpringRef = useRef(
    new SpringSolver(0, {
      stiffness: 220,
      damping: 30,
      mass: 1,
    }),
  );
  const frameHandlerRef = useRef<(now: number) => void>(() => {});

  durationRef.current = duration;
  currentTimeRef.current = currentTime;
  segmentRef.current = segment;
  isPlayingRef.current = isPlaying;
  isInteractingRef.current = isInteracting;
  disableVideoSyncRef.current = disableVideoSync;
  zoomRef.current = zoom;

  const setShowScrollbar = useCallback((nextValue: boolean) => {
    if (showScrollbarRef.current === nextValue) return;
    showScrollbarRef.current = nextValue;
    setShowScrollbarState(nextValue);
  }, []);

  const syncVisibleTimeRange = useCallback(() => {
    const viewport = viewportRef.current;
    const activeDuration = durationRef.current;
    const activeSegment = segmentRef.current;
    if (!viewport || !activeSegment || activeDuration <= 0) {
      setVisibleTimeRange(null);
      return;
    }
    const activeZoom = Math.max(zoomRef.current, MIN_TIMELINE_ZOOM);
    if (activeZoom <= MIN_TIMELINE_ZOOM + 0.001) {
      setVisibleTimeRange(null);
      return;
    }
    const visibleWidth = getVisibleContentWidth(viewport);
    const contentWidth = visibleWidth * activeZoom;
    const nextRange = buildVisibleTimeRange(
      viewport.scrollLeft,
      visibleWidth,
      contentWidth,
      activeDuration,
    );
    setVisibleTimeRange((previous) => {
      if (
        previous &&
        Math.abs(previous.startTime - nextRange.startTime) < 0.05 &&
        Math.abs(previous.endTime - nextRange.endTime) < 0.05
      ) {
        return previous;
      }
      return nextRange;
    });
  }, []);

  const scheduleVisibleRangeSync = useCallback(() => {
    if (pendingVisibleRangeRafRef.current !== null) return;
    pendingVisibleRangeRafRef.current = requestAnimationFrame(() => {
      pendingVisibleRangeRafRef.current = null;
      syncVisibleTimeRange();
    });
  }, [syncVisibleTimeRange]);

  const syncScrollbarThumb = useCallback(() => {
    const viewport = viewportRef.current;
    const timeline = timelineRef.current;
    const track = scrollbarTrackRef.current;
    const thumb = scrollbarThumbRef.current;
    const activeZoom = zoomRef.current;
    const activeDuration = durationRef.current;
    const activeSegment = segmentRef.current;

    if (!viewport || !timeline || !track || !thumb) {
      setShowScrollbar(false);
      return;
    }

    if (
      activeZoom <= MIN_TIMELINE_ZOOM ||
      activeDuration <= 0 ||
      !activeSegment
    ) {
      thumb.style.width = "0px";
      thumb.style.transform = "translateX(0px)";
      setShowScrollbar(false);
      return;
    }

    const contentWidth = getContentWidth(viewport, timeline, activeZoom);
    const visibleWidth = getVisibleContentWidth(viewport);
    const maxScroll = getMaxScroll(viewport, contentWidth);
    const trackWidth = track.clientWidth;

    if (maxScroll <= 0 || trackWidth <= 0 || visibleWidth <= 0) {
      thumb.style.width = "0px";
      thumb.style.transform = "translateX(0px)";
      setShowScrollbar(false);
      return;
    }

    const thumbWidth = Math.max(
      MIN_SCROLLBAR_THUMB_PX,
      (visibleWidth / contentWidth) * trackWidth,
    );
    const maxThumbTravel = Math.max(trackWidth - thumbWidth, 0);
    const thumbLeft =
      maxScroll > 0
        ? (clamp(viewport.scrollLeft, 0, maxScroll) / maxScroll) * maxThumbTravel
        : 0;

    thumb.style.width = `${thumbWidth}px`;
    thumb.style.transform = `translateX(${thumbLeft}px)`;
    setShowScrollbar(true);
  }, [setShowScrollbar, timelineRef]);

  const scheduleScrollbarThumbSync = useCallback(() => {
    if (pendingScrollbarRafRef.current !== null) return;
    pendingScrollbarRafRef.current = requestAnimationFrame(() => {
      pendingScrollbarRafRef.current = null;
      syncScrollbarThumb();
    });
  }, [syncScrollbarThumb]);

  const scheduleFrame = useCallback(() => {
    if (animationFrameRef.current !== null) return;
    animationFrameRef.current = requestAnimationFrame((now) => {
      animationFrameRef.current = null;
      frameHandlerRef.current(now);
    });
  }, []);

  const setProgrammaticScrollLeft = useCallback(
    (scrollLeft: number) => {
      const viewport = viewportRef.current;
      const timeline = timelineRef.current;
      if (!viewport || !timeline) return;

      const contentWidth = getContentWidth(viewport, timeline, zoomRef.current);
      const maxScroll = getMaxScroll(viewport, contentWidth);
      const clampedScrollLeft = clamp(scrollLeft, 0, maxScroll);

      if (Math.abs(viewport.scrollLeft - clampedScrollLeft) < 0.1) {
        syncScrollbarThumb();
        return;
      }

      programmaticScrollRef.current = true;
      viewport.scrollLeft = clampedScrollLeft;
      syncScrollbarThumb();

      if (releaseProgrammaticScrollRef.current !== null) {
        cancelAnimationFrame(releaseProgrammaticScrollRef.current);
      }

      releaseProgrammaticScrollRef.current = requestAnimationFrame(() => {
        programmaticScrollRef.current = false;
        releaseProgrammaticScrollRef.current = null;
      });
    },
    [syncScrollbarThumb, timelineRef],
  );

  const suppressFollow = useCallback(() => {
    const viewport = viewportRef.current;
    followSuppressedUntilRef.current = performance.now() + FOLLOW_SUPPRESS_MS;
    followActiveRef.current = false;

    if (viewport) {
      followSpringRef.current.set(viewport.scrollLeft);
    }
  }, []);

  const scrollFromTrackClientX = useCallback(
    (clientX: number) => {
      const viewport = viewportRef.current;
      const timeline = timelineRef.current;
      const track = scrollbarTrackRef.current;
      if (!viewport || !timeline || !track) return;

      const contentWidth = getContentWidth(viewport, timeline, zoomRef.current);
      const visibleWidth = getVisibleContentWidth(viewport);
      const maxScroll = getMaxScroll(viewport, contentWidth);
      const trackRect = track.getBoundingClientRect();
      const trackWidth = trackRect.width;

      if (maxScroll <= 0 || visibleWidth <= 0 || trackWidth <= 0) return;

      const thumbWidth = Math.max(
        MIN_SCROLLBAR_THUMB_PX,
        (visibleWidth / contentWidth) * trackWidth,
      );
      const maxThumbTravel = Math.max(trackWidth - thumbWidth, 0);
      const thumbLeft = clamp(
        clientX - trackRect.left - thumbWidth / 2,
        0,
        maxThumbTravel,
      );
      const nextScrollLeft =
        maxThumbTravel > 0 ? (thumbLeft / maxThumbTravel) * maxScroll : 0;

      setProgrammaticScrollLeft(nextScrollLeft);
      followSpringRef.current.set(nextScrollLeft);
    },
    [setProgrammaticScrollLeft, timelineRef],
  );

  useTimelineWheelZoom({
    viewportRef,
    timelineRef,
    durationRef,
    segmentRef,
    isInteractingRef,
    zoomRef,
    pendingWheelAnchorRef,
    programmaticScrollRef,
    releaseProgrammaticScrollRef,
    followSpringRef,
    setVisibleTimeRange,
    setZoom,
    scheduleScrollbarThumbSync,
    suppressFollow,
  });

  const {
    handleScrollbarTrackPointerDown,
    handleScrollbarThumbPointerDown,
  } = useTimelineScrollbarDrag({
    viewportRef,
    timelineRef,
    scrollbarTrackRef,
    showScrollbarRef,
    zoomRef,
    followSpringRef,
    setProgrammaticScrollLeft,
    scrollFromTrackClientX,
    suppressFollow,
  });

  frameHandlerRef.current = (now: number) => {
    const viewport = viewportRef.current;
    const timeline = timelineRef.current;
    const activeSegment = segmentRef.current;
    const activeDuration = durationRef.current;
    const activeZoom = zoomRef.current;

    if (
      !viewport ||
      !timeline ||
      !activeSegment ||
      activeDuration <= 0 ||
      activeZoom <= MIN_TIMELINE_ZOOM
    ) {
      lastFrameTimeRef.current = now;
      followActiveRef.current = false;
      syncScrollbarThumb();
      return;
    }

    const contentWidth = getContentWidth(viewport, timeline, activeZoom);
    const visibleWidth = getVisibleContentWidth(viewport);
    const maxScroll = getMaxScroll(viewport, contentWidth);

    if (maxScroll <= 0 || visibleWidth <= 0) {
      followActiveRef.current = false;
      followSpringRef.current.set(0);
      setProgrammaticScrollLeft(0);
      lastFrameTimeRef.current = now;
      return;
    }

    const dt =
      lastFrameTimeRef.current === null
        ? 1 / 60
        : clamp((now - lastFrameTimeRef.current) / 1000, 1 / 240, 0.05);
    lastFrameTimeRef.current = now;

    const followSuppressed =
      performance.now() < followSuppressedUntilRef.current;
    const shouldAllowFollow = !followSuppressed && !isInteractingRef.current;
    const shouldWatchPlayback =
      isPlayingRef.current && activeZoom > MIN_TIMELINE_ZOOM;

    if (!shouldAllowFollow) {
      followActiveRef.current = false;
      followSpringRef.current.set(viewport.scrollLeft);
      syncScrollbarThumb();
      if (shouldWatchPlayback) scheduleFrame();
      return;
    }

    const rawTime =
      isPlayingRef.current && !disableVideoSyncRef.current && videoRef.current
        ? videoRef.current.currentTime
        : currentTimeRef.current;
    const playheadTime = clampToTrimSegments(
      rawTime,
      activeSegment,
      activeDuration,
    );
    const playheadX = (playheadTime / activeDuration) * contentWidth;
    const currentScrollLeft = viewport.scrollLeft;
    const safeLeft = currentScrollLeft + visibleWidth * FOLLOW_SAFE_LEFT_RATIO;
    const safeRight = currentScrollLeft + visibleWidth * FOLLOW_SAFE_RIGHT_RATIO;

    if (
      !followActiveRef.current &&
      (playheadX < safeLeft || playheadX > safeRight)
    ) {
      followActiveRef.current = true;
      followSpringRef.current.set(currentScrollLeft);
    }

    if (followActiveRef.current) {
      const targetScrollLeft = clamp(
        playheadX - visibleWidth * FOLLOW_TARGET_RATIO,
        0,
        maxScroll,
      );
      let nextScrollLeft = followSpringRef.current.update(targetScrollLeft, dt);
      nextScrollLeft = clamp(nextScrollLeft, 0, maxScroll);

      if (Math.abs(targetScrollLeft - nextScrollLeft) <= 0.5) {
        nextScrollLeft = targetScrollLeft;
        followSpringRef.current.set(targetScrollLeft);
        if (!isPlayingRef.current) {
          followActiveRef.current = false;
        }
      }

      setProgrammaticScrollLeft(nextScrollLeft);
      scheduleFrame();
      return;
    }

    followSpringRef.current.set(currentScrollLeft);
    syncScrollbarThumb();

    if (shouldWatchPlayback) {
      scheduleFrame();
    }
  };

  useEffect(() => {
    const viewport = viewportRef.current;
    if (!viewport) return;

    const updateViewportWidth = () => {
      setViewportWidth(getVisibleContentWidth(viewport));
      syncScrollbarThumb();
      scheduleVisibleRangeSync();
    };

    updateViewportWidth();
    const resizeObserver = new ResizeObserver(() => {
      updateViewportWidth();
      scheduleFrame();
    });
    resizeObserver.observe(viewport);

    return () => resizeObserver.disconnect();
  }, [scheduleFrame, syncScrollbarThumb]);

  useEffect(() => {
    const viewport = viewportRef.current;
    if (!viewport) return;

    const handleScroll = () => {
      if (!programmaticScrollRef.current) {
        suppressFollow();
      }
      scheduleScrollbarThumbSync();
      scheduleVisibleRangeSync();
    };

    viewport.addEventListener("scroll", handleScroll, { passive: true });
    return () => viewport.removeEventListener("scroll", handleScroll);
  }, [scheduleScrollbarThumbSync, scheduleVisibleRangeSync, suppressFollow]);

  useLayoutEffect(() => {
    const viewport = viewportRef.current;
    if (!viewport) return;

    if (zoom <= MIN_TIMELINE_ZOOM) {
      pendingWheelAnchorRef.current = null;
      followActiveRef.current = false;
      followSpringRef.current.set(0);
      setProgrammaticScrollLeft(0);
      syncScrollbarThumb();
      scheduleVisibleRangeSync();
      return;
    }

    const pendingAnchor = pendingWheelAnchorRef.current;
    if (!pendingAnchor || duration <= 0) {
      syncScrollbarThumb();
      scheduleVisibleRangeSync();
      return;
    }

    const contentWidth = getContentWidth(viewport, timelineRef.current, zoom);
    const maxScroll = getMaxScroll(viewport, contentWidth);
    const anchoredScrollLeft = clamp(
      (pendingAnchor.anchorTime / duration) * contentWidth -
        pendingAnchor.pointerContentX,
      0,
      maxScroll,
    );

    setProgrammaticScrollLeft(anchoredScrollLeft);
    followSpringRef.current.set(anchoredScrollLeft);
    followActiveRef.current = false;
    pendingWheelAnchorRef.current = null;
    syncScrollbarThumb();
    scheduleVisibleRangeSync();
  }, [duration, scheduleVisibleRangeSync, setProgrammaticScrollLeft, syncScrollbarThumb, timelineRef, zoom]);

  useEffect(() => {
    if (!segment || duration <= 0) {
      pendingWheelAnchorRef.current = null;
      followActiveRef.current = false;
      followSpringRef.current.set(0);
      if (zoom !== MIN_TIMELINE_ZOOM) {
        zoomRef.current = MIN_TIMELINE_ZOOM;
        setZoom(MIN_TIMELINE_ZOOM);
      } else {
        setProgrammaticScrollLeft(0);
      }
      syncScrollbarThumb();
      scheduleVisibleRangeSync();
    }
  }, [duration, scheduleVisibleRangeSync, segment, setProgrammaticScrollLeft, syncScrollbarThumb, zoom]);

  // Schedule follow-frame on playback state changes.
  // NOTE: syncScrollbarThumb() is NOT called here — it forces layout
  // recalculation (getBoundingClientRect on the zoomed canvas) and causes
  // massive lag at 60fps. The scrollbar is synced via the scroll event
  // handler and setProgrammaticScrollLeft path instead.
  useEffect(() => {
    if (zoom <= MIN_TIMELINE_ZOOM || !segment || duration <= 0) return;
    scheduleFrame();
  }, [
    currentTime,
    duration,
    isInteracting,
    isPlaying,
    scheduleFrame,
    segment,
    zoom,
  ]);

  useEffect(() => {
    return () => {
      if (animationFrameRef.current !== null) {
        cancelAnimationFrame(animationFrameRef.current);
      }
      if (releaseProgrammaticScrollRef.current !== null) {
        cancelAnimationFrame(releaseProgrammaticScrollRef.current);
      }
      if (pendingScrollbarRafRef.current !== null) {
        cancelAnimationFrame(pendingScrollbarRafRef.current);
      }
      if (pendingVisibleRangeRafRef.current !== null) {
        cancelAnimationFrame(pendingVisibleRangeRafRef.current);
      }
    };
  }, []);

  return {
    viewportRef,
    scrollbarTrackRef,
    scrollbarThumbRef,
    zoom,
    showScrollbar: showScrollbarState,
    canvasWidth: `${Math.max(zoom, MIN_TIMELINE_ZOOM) * 100}%`,
    canvasWidthPx: viewportWidth * Math.max(zoom, MIN_TIMELINE_ZOOM),
    visibleTimeRange,
    handleScrollbarTrackPointerDown,
    handleScrollbarThumbPointerDown,
  };
}
