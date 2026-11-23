import { useCallback, useRef, useState } from "react";
import { timelineRangeKeyProps } from "./timelineKeyboard";

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

export type DragMode = "start" | "end" | "body";

interface DragState<T extends AudioTimelineSegment> {
  mode: DragMode;
  segment: T;
  pointerOffsetSec: number;
}

export interface AudioSegmentKeyOptions {
  /** Spoken name, for example: Narration "Intro". */
  name: string;
  startLabel: string;
  endLabel: string;
  /** Timeline length the segment is drawn with. */
  visibleSec: number;
  onActivate: () => void;
  onDelete?: () => void;
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

/**
 * The patch a pointer at `pointerTime` makes to `segment` while dragging its
 * body or one edge. Edges trim the source (inPoint/outPoint) at the segment's
 * playback rate; the body keeps its length and stays inside the timeline.
 */
export function resolveAudioSegmentEdit(
  segment: AudioTimelineSegment,
  mode: DragMode,
  pointerTime: number,
  pointerOffsetSec: number,
  duration: number,
): Partial<AudioTimelineSegment> {
  const rate = getRate(segment);
  const sourceMin = MIN_SOURCE_SEC * rate;
  const sourceDuration = Math.max(segment.duration, segment.outPoint, segment.inPoint);

  if (mode === "body") {
    const maxStart = Math.max(0, duration - getTimelineLength(segment));
    return { startTime: Math.max(0, Math.min(maxStart, pointerTime - pointerOffsetSec)) };
  }

  if (mode === "start") {
    const maxTimelineStart = segment.startTime + getTimelineLength(segment) - MIN_SOURCE_SEC;
    const requestedStart = Math.max(0, Math.min(maxTimelineStart, pointerTime));
    const requestedSourceIn = segment.inPoint + (requestedStart - segment.startTime) * rate;
    const inPoint = Math.max(0, Math.min(segment.outPoint - sourceMin, requestedSourceIn));
    return { startTime: segment.startTime + (inPoint - segment.inPoint) / rate, inPoint };
  }

  const maxTimelineEnd = segment.startTime + Math.max(0, (sourceDuration - segment.inPoint) / rate);
  const requestedEnd = Math.max(
    segment.startTime + MIN_SOURCE_SEC,
    Math.min(duration, Math.min(maxTimelineEnd, pointerTime)),
  );
  const outPoint = Math.max(
    segment.inPoint + sourceMin,
    Math.min(sourceDuration, segment.inPoint + (requestedEnd - segment.startTime) * rate),
  );
  return { outPoint };
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
  const pendingPointerTimeRef = useRef<number | null>(null);
  const dragFrameRef = useRef<number | null>(null);

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

  const applyPointerTime = useCallback(
    (pointerTime: number) => {
      const drag = dragRef.current;
      if (!drag || !onUpdateSegment) return;
      didMoveRef.current = true;
      onUpdateSegment(
        drag.segment.id,
        resolveAudioSegmentEdit(drag.segment, drag.mode, pointerTime, drag.pointerOffsetSec, duration) as Partial<T>,
      );
    },
    [duration, onUpdateSegment],
  );

  /** Keyboard edit: moves the segment or one edge by `deltaSec` as one undo step. */
  const nudgeAudioSegment = useCallback(
    (segment: T, mode: DragMode, deltaSec: number) => {
      if (!onUpdateSegment) return;
      const edgeTime =
        mode === "end" ? segment.startTime + getTimelineLength(segment) : segment.startTime;
      beginBatch?.();
      onUpdateSegment(
        segment.id,
        resolveAudioSegmentEdit(segment, mode, edgeTime + deltaSec, 0, duration) as Partial<T>,
      );
      commitBatch?.();
      onCommitSegments?.();
    },
    [beginBatch, commitBatch, duration, onCommitSegments, onUpdateSegment],
  );

  const handlePointerMove = useCallback(
    (event: React.PointerEvent<HTMLElement>) => {
      if (!dragRef.current || !onUpdateSegment) return;
      const pointerTime = getTimeFromEvent(event);
      if (pointerTime === null) return;
      pendingPointerTimeRef.current = pointerTime;
      if (dragFrameRef.current !== null) return;
      dragFrameRef.current = requestAnimationFrame(() => {
        dragFrameRef.current = null;
        const pendingPointerTime = pendingPointerTimeRef.current;
        pendingPointerTimeRef.current = null;
        if (pendingPointerTime !== null) applyPointerTime(pendingPointerTime);
      });
    },
    [applyPointerTime, getTimeFromEvent, onUpdateSegment],
  );

  const handlePointerUp = useCallback(
    (event: React.PointerEvent<HTMLElement>) => {
      if (!dragRef.current) return;
      if (dragFrameRef.current !== null) {
        cancelAnimationFrame(dragFrameRef.current);
        dragFrameRef.current = null;
      }
      const pendingPointerTime = pendingPointerTimeRef.current;
      pendingPointerTimeRef.current = null;
      if (pendingPointerTime !== null) applyPointerTime(pendingPointerTime);
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
    [applyPointerTime, commitBatch, onCommitSegments],
  );

  /** Focus, roles and keys for one audio segment body and its two trim edges. */
  const getAudioSegmentKeys = (segment: T, options: AudioSegmentKeyOptions) =>
    timelineRangeKeyProps({
      name: options.name,
      startLabel: options.startLabel,
      endLabel: options.endLabel,
      range: { startTime: segment.startTime, endTime: segment.startTime + options.visibleSec },
      duration,
      nudge: onUpdateSegment && ((edge, deltaSec) => nudgeAudioSegment(segment, edge, deltaSec)),
      onActivate: options.onActivate,
      onDelete: options.onDelete,
    });

  return {
    isDraggingAudioSegment,
    handleAudioSegmentPointerDown: handlePointerDown,
    handleAudioSegmentPointerMove: handlePointerMove,
    handleAudioSegmentPointerUp: handlePointerUp,
    nudgeAudioSegment,
    getAudioSegmentKeys,
  };
}
