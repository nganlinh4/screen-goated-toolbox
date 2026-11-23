/**
 * Shared window-level pointer drag for timeline handles (curve points, zoom-block
 * edges, compact audio blocks). It listens to pointer events, so touch and pen
 * drags work. It re-reads the track rect on every move, so follow-playhead
 * scrolling during a drag does not shift the value. It ends on pointercancel,
 * and while any drag is active it reports the timeline as interacting so
 * follow-playhead pauses.
 */
let activeDragCount = 0;

export function isTimelinePointerDragActive(): boolean {
  return activeDragCount > 0;
}

export interface TimelinePointerDragOptions {
  /** Element whose current bounding rect maps clientX to timeline time. */
  element: HTMLElement;
  onMove: (event: PointerEvent, rect: DOMRect) => void;
  /** Called once when the drag ends; `cancelled` is true for pointercancel. */
  onEnd: (cancelled: boolean) => void;
}

/** Starts a drag and returns a function that ends it early (e.g. on unmount). */
export function startTimelinePointerDrag({
  element,
  onMove,
  onEnd,
}: TimelinePointerDragOptions): () => void {
  let finished = false;
  activeDragCount += 1;

  const handleMove = (event: PointerEvent) => {
    onMove(event, element.getBoundingClientRect());
  };
  const finish = (cancelled: boolean) => {
    if (finished) return;
    finished = true;
    activeDragCount = Math.max(0, activeDragCount - 1);
    window.removeEventListener("pointermove", handleMove);
    window.removeEventListener("pointerup", handleUp);
    window.removeEventListener("pointercancel", handleCancel);
    onEnd(cancelled);
  };
  const handleUp = () => finish(false);
  const handleCancel = () => finish(true);

  window.addEventListener("pointermove", handleMove);
  window.addEventListener("pointerup", handleUp);
  window.addEventListener("pointercancel", handleCancel);
  return () => finish(true);
}
