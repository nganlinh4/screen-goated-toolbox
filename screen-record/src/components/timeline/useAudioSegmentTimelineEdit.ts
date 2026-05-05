import { useCallback, useRef, useState } from "react";

const EDGE_HANDLE_PX = 8;
const MIN_SOURCE_SEC = 0.05;

type AudioTimelineSegment = {
  id: string;
  duration: number;
  startTime: number;
  inPoint: number;
  outPoint: number;
  playbackRate?: number;
};

type DragMode = "start" | "end" | "body";

interface DragState<T extends AudioTimelineSegment> {
  mode: DragMode;
  segment: T;
  pointerOffsetSec: number;
}

interface UseAudioSegmentTimelineEditOptions<T extends AudioTimelineSegment> {
  duration: number;
  onUpdateSegment?: (id: string, patch: Partial<T>) => void;
  beginBatch?: () => void;
  commitBatch?: () => void;
  onCommitSegments?: () => void;
}

function getRate(segment: AudioTimelineSegment) {
  return segment.playbackRate && segment.playbackRate > 0
    ? segment.playbackRate
    : 1;
}

function getTimelineLength(segment: AudioTimelineSegment) {
  return Math.max((segment.outPoint - segment.inPoint) / getRate(segment), MIN_SOURCE_SEC);
}

export function useAudioSegmentTimelineEdit<T extends AudioTimelineSegment>({
  duration,
  onUpdateSegment,
  beginBatch,
  commitBatch,
  onCommitSegments,
}: UseAudioSegmentTimelineEditOptions<T>) {
  const [isDraggingAudioSegment, setIsDraggingAudioSegment] = useState(false);
  const dragRef = useRef<DragState<T> | null>(null);
  const didMoveRef = useRef(false);

  const getTimeFromEvent = useCallback(
    (event: React.PointerEvent<HTMLElement>) => {
      const parent = event.currentTarget.parentElement;
      if (!parent) return null;
      const rect = parent.getBoundingClientRect();
      if (rect.width <= 0) return null;
      const x = Math.max(0, Math.min(rect.width, event.clientX - rect.left));
      return (x / rect.width) * Math.max(duration, 0.001);
    },
    [duration],
  );

  const handlePointerDown = useCallback(
    (event: React.PointerEvent<HTMLElement>, segment: T) => {
      if (!onUpdateSegment || event.button !== 0 || event.ctrlKey || event.shiftKey) return false;
      const rect = event.currentTarget.getBoundingClientRect();
      const x = event.clientX - rect.left;
      const mode: DragMode =
        x <= EDGE_HANDLE_PX
          ? "start"
          : x >= rect.width - EDGE_HANDLE_PX
            ? "end"
            : "body";
      const pointerTime = getTimeFromEvent(event);
      if (pointerTime === null) return false;
      beginBatch?.();
      dragRef.current = {
        mode,
        segment,
        pointerOffsetSec: Math.max(0, pointerTime - segment.startTime),
      };
      didMoveRef.current = false;
      setIsDraggingAudioSegment(true);
      event.currentTarget.setPointerCapture(event.pointerId);
      event.preventDefault();
      event.stopPropagation();
      return true;
    },
    [beginBatch, getTimeFromEvent, onUpdateSegment],
  );

  const handlePointerMove = useCallback(
    (event: React.PointerEvent<HTMLElement>) => {
      const drag = dragRef.current;
      if (!drag || !onUpdateSegment) return;
      const pointerTime = getTimeFromEvent(event);
      if (pointerTime === null) return;
      const { segment, mode } = drag;
      const rate = getRate(segment);
      const sourceMin = MIN_SOURCE_SEC * rate;
      const sourceDuration = Math.max(segment.duration, segment.outPoint, segment.inPoint);

      didMoveRef.current = true;
      if (mode === "body") {
        const timelineLength = getTimelineLength(segment);
        const maxStart = Math.max(0, duration - timelineLength);
        const startTime = Math.max(0, Math.min(maxStart, pointerTime - drag.pointerOffsetSec));
        onUpdateSegment(segment.id, { startTime } as Partial<T>);
        return;
      }

      if (mode === "start") {
        const maxTimelineStart = segment.startTime + getTimelineLength(segment) - MIN_SOURCE_SEC;
        const requestedStart = Math.max(0, Math.min(maxTimelineStart, pointerTime));
        const requestedSourceIn =
          segment.inPoint + (requestedStart - segment.startTime) * rate;
        const inPoint = Math.max(
          0,
          Math.min(segment.outPoint - sourceMin, requestedSourceIn),
        );
        const startTime = segment.startTime + (inPoint - segment.inPoint) / rate;
        onUpdateSegment(segment.id, { startTime, inPoint } as Partial<T>);
        return;
      }

      const maxTimelineEnd =
        segment.startTime + Math.max(0, (sourceDuration - segment.inPoint) / rate);
      const requestedEnd = Math.max(
        segment.startTime + MIN_SOURCE_SEC,
        Math.min(duration, Math.min(maxTimelineEnd, pointerTime)),
      );
      const outPoint = Math.max(
        segment.inPoint + sourceMin,
        Math.min(sourceDuration, segment.inPoint + (requestedEnd - segment.startTime) * rate),
      );
      onUpdateSegment(segment.id, { outPoint } as Partial<T>);
    },
    [duration, getTimeFromEvent, onUpdateSegment],
  );

  const handlePointerUp = useCallback(
    (event: React.PointerEvent<HTMLElement>) => {
      if (!dragRef.current) return;
      dragRef.current = null;
      setIsDraggingAudioSegment(false);
      try {
        event.currentTarget.releasePointerCapture(event.pointerId);
      } catch {
        /* pointer capture may already be released */
      }
      commitBatch?.();
      if (didMoveRef.current) onCommitSegments?.();
      event.stopPropagation();
    },
    [commitBatch, onCommitSegments],
  );

  return {
    isDraggingAudioSegment,
    handleAudioSegmentPointerDown: handlePointerDown,
    handleAudioSegmentPointerMove: handlePointerMove,
    handleAudioSegmentPointerUp: handlePointerUp,
  };
}
