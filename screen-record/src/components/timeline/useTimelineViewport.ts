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

const MIN_TIMELINE_ZOOM = 1;
const MAX_TIMELINE_ZOOM = 12;
const WHEEL_ZOOM_SENSITIVITY = 0.002;
const FOLLOW_SUPPRESS_MS = 900;
const FOLLOW_SAFE_LEFT_RATIO = 0.2;
const FOLLOW_SAFE_RIGHT_RATIO = 0.8;
const FOLLOW_TARGET_RATIO = 0.6;
const VIEWPORT_BLEED_PX = 16;
const MIN_SCROLLBAR_THUMB_PX = 36;

function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}

function getVisibleContentWidth(viewport: HTMLDivElement): number {
  return Math.max(viewport.clientWidth - VIEWPORT_BLEED_PX * 2, 1);
}

function getContentWidth(
  viewport: HTMLDivElement,
  timeline: HTMLDivElement | null,
  zoom: number,
): number {
  const measuredWidth = timeline?.getBoundingClientRect().width ?? 0;
  if (measuredWidth > 0) return measuredWidth;
  return getVisibleContentWidth(viewport) * zoom;
}

function getMaxScroll(viewport: HTMLDivElement, contentWidth: number): number {
  return Math.max(0, contentWidth - getVisibleContentWidth(viewport));
}

interface UseTimelineViewportOptions {
  duration: number;
  currentTime: number;
  segment: VideoSegment | null;
  timelineRef: RefObject<HTMLDivElement>;
  videoRef: RefObject<HTMLVideoElement>;
  isPlaying: boolean;
  isInteracting: boolean;
}

interface UseTimelineViewportResult {
  viewportRef: RefObject<HTMLDivElement>;
  scrollbarTrackRef: RefObject<HTMLDivElement>;
  scrollbarThumbRef: RefObject<HTMLDivElement>;
  zoom: number;
  showScrollbar: boolean;
  canvasWidth: string;
  canvasWidthPx: number;
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
}: UseTimelineViewportOptions): UseTimelineViewportResult {
  const viewportRef = useRef<HTMLDivElement>(null);
  const scrollbarTrackRef = useRef<HTMLDivElement>(null);
  const scrollbarThumbRef = useRef<HTMLDivElement>(null);
  const [zoom, setZoom] = useState(1);
  const [viewportWidth, setViewportWidth] = useState(0);
  const [showScrollbarState, setShowScrollbarState] = useState(false);
  const animationFrameRef = useRef<number | null>(null);
  const releaseProgrammaticScrollRef = useRef<number | null>(null);
  const lastFrameTimeRef = useRef<number | null>(null);
  const pendingWheelAnchorRef = useRef<{
    anchorTime: number;
    pointerContentX: number;
  } | null>(null);
  const durationRef = useRef(duration);
  const currentTimeRef = useRef(currentTime);
  const segmentRef = useRef(segment);
  const isPlayingRef = useRef(isPlaying);
  const isInteractingRef = useRef(isInteracting);
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
  zoomRef.current = zoom;

  const setShowScrollbar = useCallback((nextValue: boolean) => {
    if (showScrollbarRef.current === nextValue) return;
    showScrollbarRef.current = nextValue;
    setShowScrollbarState(nextValue);
  }, []);

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
      isPlayingRef.current && videoRef.current
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
      syncScrollbarThumb();
    };

    viewport.addEventListener("scroll", handleScroll, { passive: true });
    return () => viewport.removeEventListener("scroll", handleScroll);
  }, [suppressFollow, syncScrollbarThumb]);

  useLayoutEffect(() => {
    const viewport = viewportRef.current;
    if (!viewport) return;

    if (zoom <= MIN_TIMELINE_ZOOM) {
      pendingWheelAnchorRef.current = null;
      followActiveRef.current = false;
      followSpringRef.current.set(0);
      setProgrammaticScrollLeft(0);
      syncScrollbarThumb();
      return;
    }

    const pendingAnchor = pendingWheelAnchorRef.current;
    if (!pendingAnchor || duration <= 0) {
      syncScrollbarThumb();
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
  }, [duration, setProgrammaticScrollLeft, syncScrollbarThumb, timelineRef, zoom]);

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
    }
  }, [duration, segment, setProgrammaticScrollLeft, syncScrollbarThumb, zoom]);

  useEffect(() => {
    syncScrollbarThumb();
    if (zoom <= MIN_TIMELINE_ZOOM || !segment || duration <= 0) return;
    scheduleFrame();
  }, [
    currentTime,
    duration,
    isInteracting,
    isPlaying,
    scheduleFrame,
    segment,
    syncScrollbarThumb,
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
    };
  }, []);

  // Wheel zoom handler — registered natively with { passive: false } so
  // preventDefault() works without triggering "passive event listener" warnings.
  const wheelHandlerRef = useRef<(event: globalThis.WheelEvent) => void>(() => {});
  wheelHandlerRef.current = (event: globalThis.WheelEvent) => {
    const viewport = viewportRef.current;
    const activeSegment = segmentRef.current;
    const activeDuration = durationRef.current;
    const activeZoom = zoomRef.current;

    if (
      !viewport ||
      !activeSegment ||
      activeDuration <= 0 ||
      isInteractingRef.current
    ) {
      return;
    }

    if (Math.abs(event.deltaY) <= Math.abs(event.deltaX)) {
      return;
    }

    event.preventDefault();

    const visibleWidth = getVisibleContentWidth(viewport);
    const viewportRect = viewport.getBoundingClientRect();
    const pointerContentX = clamp(
      event.clientX - viewportRect.left - VIEWPORT_BLEED_PX,
      0,
      visibleWidth,
    );
    const contentWidth = getContentWidth(viewport, timelineRef.current, activeZoom);
    const anchorTime = clamp(
      ((viewport.scrollLeft + pointerContentX) / Math.max(contentWidth, 1)) *
        activeDuration,
      0,
      activeDuration,
    );
    const nextZoom = clamp(
      activeZoom - event.deltaY * WHEEL_ZOOM_SENSITIVITY * activeZoom,
      MIN_TIMELINE_ZOOM,
      MAX_TIMELINE_ZOOM,
    );

    if (Math.abs(nextZoom - activeZoom) < 0.001) return;

    pendingWheelAnchorRef.current = {
      anchorTime,
      pointerContentX,
    };
    suppressFollow();
    zoomRef.current = nextZoom;
    setZoom(nextZoom);
  };

  useEffect(() => {
    const viewport = viewportRef.current;
    if (!viewport) return;

    const handler = (event: globalThis.WheelEvent) => {
      wheelHandlerRef.current(event);
    };
    viewport.addEventListener("wheel", handler, { passive: false });
    return () => viewport.removeEventListener("wheel", handler);
  }, []);

  const handleScrollbarTrackPointerDown = useCallback(
    (event: PointerEvent<HTMLDivElement>) => {
      if (!showScrollbarRef.current) return;
      event.preventDefault();
      suppressFollow();
      scrollFromTrackClientX(event.clientX);

      const handleMove = (moveEvent: globalThis.PointerEvent) => {
        scrollFromTrackClientX(moveEvent.clientX);
      };
      const handleUp = () => {
        window.removeEventListener("pointermove", handleMove);
        window.removeEventListener("pointerup", handleUp);
        window.removeEventListener("pointercancel", handleUp);
      };

      window.addEventListener("pointermove", handleMove);
      window.addEventListener("pointerup", handleUp);
      window.addEventListener("pointercancel", handleUp);
    },
    [scrollFromTrackClientX, suppressFollow],
  );

  const handleScrollbarThumbPointerDown = useCallback(
    (event: PointerEvent<HTMLDivElement>) => {
      const viewport = viewportRef.current;
      const timeline = timelineRef.current;
      const track = scrollbarTrackRef.current;
      if (!viewport || !timeline || !track || !showScrollbarRef.current) return;

      event.preventDefault();
      event.stopPropagation();
      suppressFollow();

      const startClientX = event.clientX;
      const startScrollLeft = viewport.scrollLeft;
      const contentWidth = getContentWidth(viewport, timeline, zoomRef.current);
      const visibleWidth = getVisibleContentWidth(viewport);
      const maxScroll = getMaxScroll(viewport, contentWidth);
      const trackWidth = track.clientWidth;
      const thumbWidth = Math.max(
        MIN_SCROLLBAR_THUMB_PX,
        (visibleWidth / contentWidth) * trackWidth,
      );
      const maxThumbTravel = Math.max(trackWidth - thumbWidth, 0);

      const handleMove = (moveEvent: globalThis.PointerEvent) => {
        const deltaX = moveEvent.clientX - startClientX;
        const nextScrollLeft =
          maxThumbTravel > 0
            ? startScrollLeft + (deltaX / maxThumbTravel) * maxScroll
            : startScrollLeft;
        setProgrammaticScrollLeft(nextScrollLeft);
        followSpringRef.current.set(clamp(nextScrollLeft, 0, maxScroll));
      };
      const handleUp = () => {
        window.removeEventListener("pointermove", handleMove);
        window.removeEventListener("pointerup", handleUp);
        window.removeEventListener("pointercancel", handleUp);
      };

      window.addEventListener("pointermove", handleMove);
      window.addEventListener("pointerup", handleUp);
      window.addEventListener("pointercancel", handleUp);
    },
    [setProgrammaticScrollLeft, suppressFollow, timelineRef],
  );

  return {
    viewportRef,
    scrollbarTrackRef,
    scrollbarThumbRef,
    zoom,
    showScrollbar: showScrollbarState,
    canvasWidth: `${Math.max(zoom, MIN_TIMELINE_ZOOM) * 100}%`,
    canvasWidthPx: viewportWidth * Math.max(zoom, MIN_TIMELINE_ZOOM),
    handleScrollbarTrackPointerDown,
    handleScrollbarThumbPointerDown,
  };
}
