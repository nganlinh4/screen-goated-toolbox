import {
  useCallback,
  useEffect,
  useRef,
  type Dispatch,
  type MutableRefObject,
  type RefObject,
  type SetStateAction,
} from "react";
import type { SpringSolver } from "@/lib/spring";
import type { VideoSegment } from "@/types/video";
import type { TimelineVisibleRange } from "./SegmentBlocksCanvas";
import {
  buildVisibleTimeRange,
  clamp,
  getContentWidth,
  getMaxScroll,
  getVisibleContentWidth,
  MAX_TIMELINE_ZOOM,
  MIN_TIMELINE_ZOOM,
  type TimelineWheelAnchor,
  VIEWPORT_BLEED_PX,
  WHEEL_ZOOM_SENSITIVITY,
} from "./timelineViewportMath";

interface UseTimelineWheelZoomOptions {
  viewportRef: RefObject<HTMLDivElement | null>;
  timelineRef: RefObject<HTMLDivElement | null>;
  durationRef: MutableRefObject<number>;
  segmentRef: MutableRefObject<VideoSegment | null>;
  isInteractingRef: MutableRefObject<boolean>;
  zoomRef: MutableRefObject<number>;
  pendingWheelAnchorRef: MutableRefObject<TimelineWheelAnchor | null>;
  programmaticScrollRef: MutableRefObject<boolean>;
  releaseProgrammaticScrollRef: MutableRefObject<number | null>;
  followSpringRef: MutableRefObject<SpringSolver>;
  setVisibleTimeRange: Dispatch<SetStateAction<TimelineVisibleRange | null>>;
  setZoom: Dispatch<SetStateAction<number>>;
  scheduleScrollbarThumbSync: () => void;
  suppressFollow: () => void;
}

export function useTimelineWheelZoom({
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
}: UseTimelineWheelZoomOptions): void {
  const pendingZoomRafRef = useRef<number | null>(null);
  const commitPendingWheelZoom = useCallback(() => {
    pendingZoomRafRef.current = null;
    const activeViewport = viewportRef.current;
    const activeDurationForRange = durationRef.current;
    const activeZoomForRange = zoomRef.current;
    const pendingAnchor = pendingWheelAnchorRef.current;
    if (activeViewport && segmentRef.current && activeDurationForRange > 0) {
      const visibleWidthForRange = getVisibleContentWidth(activeViewport);
      const contentWidthForRange =
        visibleWidthForRange *
        Math.max(activeZoomForRange, MIN_TIMELINE_ZOOM);
      const scrollLeftForRange = pendingAnchor
        ? clamp(
            (pendingAnchor.anchorTime / activeDurationForRange) *
              contentWidthForRange -
              pendingAnchor.pointerContentX,
            0,
            getMaxScroll(activeViewport, contentWidthForRange),
          )
        : activeViewport.scrollLeft;
      setVisibleTimeRange(
        activeZoomForRange <= MIN_TIMELINE_ZOOM + 0.001
          ? null
          : buildVisibleTimeRange(
              scrollLeftForRange,
              visibleWidthForRange,
              contentWidthForRange,
              activeDurationForRange,
            ),
      );
    }
    setZoom(zoomRef.current);
  }, [
    durationRef,
    pendingWheelAnchorRef,
    segmentRef,
    setVisibleTimeRange,
    setZoom,
    viewportRef,
    zoomRef,
  ]);

  const wheelHandlerRef = useRef<(event: globalThis.WheelEvent) => void>(
    () => {},
  );
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
    const contentWidth = getContentWidth(
      viewport,
      timelineRef.current,
      activeZoom,
    );
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
    const canvas = timelineRef.current?.parentElement;
    const nextContentWidth =
      visibleWidth * Math.max(nextZoom, MIN_TIMELINE_ZOOM);
    const nextScrollLeft = clamp(
      (anchorTime / activeDuration) * nextContentWidth - pointerContentX,
      0,
      getMaxScroll(viewport, nextContentWidth),
    );
    if (canvas instanceof HTMLElement) {
      canvas.style.width = `${Math.max(nextZoom, MIN_TIMELINE_ZOOM) * 100}%`;
    }
    programmaticScrollRef.current = true;
    viewport.scrollLeft = nextScrollLeft;
    followSpringRef.current.set(nextScrollLeft);
    scheduleScrollbarThumbSync();
    if (releaseProgrammaticScrollRef.current !== null) {
      cancelAnimationFrame(releaseProgrammaticScrollRef.current);
    }
    releaseProgrammaticScrollRef.current = requestAnimationFrame(() => {
      programmaticScrollRef.current = false;
      releaseProgrammaticScrollRef.current = null;
    });
    if (pendingZoomRafRef.current === null) {
      pendingZoomRafRef.current = requestAnimationFrame(commitPendingWheelZoom);
    }
  };

  useEffect(() => {
    const viewport = viewportRef.current;
    if (!viewport) return;

    const handler = (event: globalThis.WheelEvent) => {
      wheelHandlerRef.current(event);
    };
    viewport.addEventListener("wheel", handler, { passive: false });
    return () => viewport.removeEventListener("wheel", handler);
  }, [viewportRef]);

  useEffect(() => {
    return () => {
      if (pendingZoomRafRef.current !== null) {
        cancelAnimationFrame(pendingZoomRafRef.current);
      }
    };
  }, []);
}
