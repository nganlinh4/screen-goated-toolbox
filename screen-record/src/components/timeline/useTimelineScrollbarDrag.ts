import {
  useCallback,
  type MutableRefObject,
  type PointerEvent,
  type RefObject,
} from "react";
import type { SpringSolver } from "@/lib/spring";
import {
  clamp,
  getContentWidth,
  getMaxScroll,
  getVisibleContentWidth,
  MIN_SCROLLBAR_THUMB_PX,
} from "./timelineViewportMath";

interface UseTimelineScrollbarDragOptions {
  viewportRef: RefObject<HTMLDivElement | null>;
  timelineRef: RefObject<HTMLDivElement | null>;
  scrollbarTrackRef: RefObject<HTMLDivElement | null>;
  showScrollbarRef: MutableRefObject<boolean>;
  zoomRef: MutableRefObject<number>;
  followSpringRef: MutableRefObject<SpringSolver>;
  setProgrammaticScrollLeft: (scrollLeft: number) => void;
  scrollFromTrackClientX: (clientX: number) => void;
  suppressFollow: () => void;
}

export function useTimelineScrollbarDrag({
  viewportRef,
  timelineRef,
  scrollbarTrackRef,
  showScrollbarRef,
  zoomRef,
  followSpringRef,
  setProgrammaticScrollLeft,
  scrollFromTrackClientX,
  suppressFollow,
}: UseTimelineScrollbarDragOptions) {
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
    [scrollFromTrackClientX, showScrollbarRef, suppressFollow],
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
    [
      followSpringRef,
      scrollbarTrackRef,
      setProgrammaticScrollLeft,
      showScrollbarRef,
      suppressFollow,
      timelineRef,
      viewportRef,
      zoomRef,
    ],
  );

  return {
    handleScrollbarTrackPointerDown,
    handleScrollbarThumbPointerDown,
  };
}
