import { useState, useCallback, useRef } from 'react';
import type { CursorVisibilitySegment, VideoSegment } from '@/types/video';
import { clampSegmentEdge, splitVisibilitySegmentAtTime } from './trackDragUtils';

/**
 * Per-track configuration for {@link useVisibilitySegmentDrag}.
 *
 * The keystroke, pointer, and webcam tracks share an identical drag/click
 * machine over a `CursorVisibilitySegment[]`; they differ only in which
 * segment-array field they read/write and a few side effects. Each track
 * supplies these callbacks; the hook owns the shared drag state and logic.
 */
export interface VisibilitySegmentDragConfig {
  /** Total timeline duration (seconds). */
  duration: number;
  /** Current segment, or null. */
  segment: VideoSegment | null;
  /** Store write-back. */
  setSegment: (segment: VideoSegment | null) => void;
  /** Maps a clientX to a timeline time, or null when unavailable. */
  getTimeFromClientX: (clientX: number) => number | null;
  beginBatch: () => void;
  commitBatch: () => void;
  /**
   * Optional drag-start guard. When provided and it returns false, the drag
   * start is aborted before `beginBatch()` runs. Tracks that always start a
   * batch (pointer, webcam) omit this.
   */
  dragStartGuard?: (segment: VideoSegment | null) => boolean;
  /** Snapshot of the track's segments at drag start (already duration-clamped + cloned), or null. */
  snapshotOriginals: (segment: VideoSegment | null) => CursorVisibilitySegment[] | null;
  /** Side effects fired on drag start (set/clear editing ids). */
  onDragStart: (id: string) => void;
  /** Write back the (merged + duration-clamped) segments during a drag move. */
  writeDuringDrag: (segment: VideoSegment, clampedSegments: CursorVisibilitySegment[]) => VideoSegment;
  /** Resolve the segments to split on click, or null to bail. */
  getSegmentsForSplit: (segment: VideoSegment | null) => CursorVisibilitySegment[] | null;
  /** Write back the post-split segments on click. */
  setSegmentsAfterSplit: (segment: VideoSegment, nextSegs: CursorVisibilitySegment[]) => VideoSegment;
  /** Side effects fired after a successful split (clear editing ids). */
  onSplit: () => void;
}

export function useVisibilitySegmentDrag(config: VisibilitySegmentDragConfig) {
  const {
    duration,
    segment,
    setSegment,
    getTimeFromClientX,
    beginBatch,
    commitBatch,
    dragStartGuard,
    snapshotOriginals,
    onDragStart,
    writeDuringDrag,
    getSegmentsForSplit,
    setSegmentsAfterSplit,
    onSplit,
  } = config;

  const [isDraggingStart, setIsDraggingStart] = useState(false);
  const [isDraggingEnd, setIsDraggingEnd] = useState(false);
  const [isDraggingBody, setIsDraggingBody] = useState(false);
  const [draggingId, setDraggingId] = useState<string | null>(null);
  const dragOffsetRef = useRef(0);
  const dragOriginals = useRef<CursorVisibilitySegment[] | null>(null);
  const dragDidMove = useRef(false);

  const handleDragStart = useCallback((id: string, type: 'start' | 'end' | 'body', offset?: number) => {
    if (dragStartGuard && !dragStartGuard(segment)) return;
    beginBatch();
    dragDidMove.current = false;
    dragOriginals.current = snapshotOriginals(segment);
    setDraggingId(id);
    onDragStart(id);
    if (type === 'start') setIsDraggingStart(true);
    else if (type === 'end') setIsDraggingEnd(true);
    else if (type === 'body') {
      setIsDraggingBody(true);
      if (offset !== undefined) dragOffsetRef.current = offset;
    }
  }, [beginBatch, segment, dragStartGuard, snapshotOriginals, onDragStart]);

  const handleDrag = useCallback((clientX: number) => {
    if ((!isDraggingStart && !isDraggingEnd && !isDraggingBody) || !draggingId || !segment) return;
    const originals = dragOriginals.current;
    if (!originals) return;
    const newTime = getTimeFromClientX(clientX);
    if (newTime === null) return;
    dragDidMove.current = true;

    const modified = originals.map((seg) => {
      if (seg.id !== draggingId) return { ...seg };
      if (isDraggingStart) {
        return { ...seg, startTime: clampSegmentEdge(seg, 'start', newTime, duration) };
      }
      if (isDraggingEnd) {
        return { ...seg, endTime: clampSegmentEdge(seg, 'end', newTime, duration) };
      }
      if (isDraggingBody) {
        const segDuration = seg.endTime - seg.startTime;
        let nextStart = newTime - dragOffsetRef.current;
        if (nextStart < 0) nextStart = 0;
        if (nextStart + segDuration > duration) nextStart = duration - segDuration;
        return { ...seg, startTime: nextStart, endTime: nextStart + segDuration };
      }
      return { ...seg };
    });

    setSegment(writeDuringDrag(segment, modified));
  }, [
    isDraggingStart,
    isDraggingEnd,
    isDraggingBody,
    draggingId,
    segment,
    getTimeFromClientX,
    setSegment,
    duration,
    writeDuringDrag,
  ]);

  const handleClick = useCallback((id: string, splitTime: number) => {
    if (isDraggingStart || isDraggingEnd || isDraggingBody) return;
    if (dragDidMove.current) {
      dragDidMove.current = false;
      return;
    }
    const segments = getSegmentsForSplit(segment);
    if (!segments) return;

    const nextSegs = splitVisibilitySegmentAtTime(segments, id, splitTime);
    if (!nextSegs) return;

    beginBatch();
    setSegment(setSegmentsAfterSplit(segment as VideoSegment, nextSegs));
    onSplit();
    commitBatch();
  }, [
    isDraggingStart,
    isDraggingEnd,
    isDraggingBody,
    segment,
    setSegment,
    beginBatch,
    commitBatch,
    getSegmentsForSplit,
    setSegmentsAfterSplit,
    onSplit,
  ]);

  const reset = useCallback(() => {
    setIsDraggingStart(false);
    setIsDraggingEnd(false);
    setIsDraggingBody(false);
    setDraggingId(null);
    dragOriginals.current = null;
  }, []);

  return {
    isDraggingStart,
    isDraggingEnd,
    isDraggingBody,
    draggingId,
    handleDragStart,
    handleDrag,
    handleClick,
    reset,
  };
}
