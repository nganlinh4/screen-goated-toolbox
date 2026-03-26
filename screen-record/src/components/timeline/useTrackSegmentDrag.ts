import { useState, useCallback, useRef } from 'react';
import { VideoSegment, CursorVisibilitySegment } from '@/types/video';
import { clampVisibilitySegmentsToDuration, mergePointerSegments } from '@/lib/cursorHiding';
import { getKeystrokeVisibilitySegmentsForMode, withKeystrokeVisibilitySegmentsForMode } from '@/lib/keystrokeVisibility';

interface UseTrackSegmentDragOptions {
  duration: number;
  segment: VideoSegment | null;
  setSegment: (segment: VideoSegment | null) => void;
  setEditingTextId: (id: string | null) => void;
  setEditingKeystrokeId?: (id: string | null) => void;
  setEditingPointerId?: (id: string | null) => void;
  setActivePanel: (panel: 'zoom' | 'background' | 'cursor' | 'text') => void;
  getTimeFromClientX: (clientX: number) => number | null;
  beginBatch: () => void;
  commitBatch: () => void;
}

export function useTrackSegmentDrag({
  duration,
  segment,
  setSegment,
  setEditingTextId,
  setEditingKeystrokeId,
  setEditingPointerId,
  setActivePanel,
  getTimeFromClientX,
  beginBatch,
  commitBatch,
}: UseTrackSegmentDragOptions) {
  const [isDraggingTextStart, setIsDraggingTextStart] = useState(false);
  const [isDraggingTextEnd, setIsDraggingTextEnd] = useState(false);
  const [isDraggingTextBody, setIsDraggingTextBody] = useState(false);
  const [isDraggingKeystrokeStart, setIsDraggingKeystrokeStart] = useState(false);
  const [isDraggingKeystrokeEnd, setIsDraggingKeystrokeEnd] = useState(false);
  const [isDraggingKeystrokeBody, setIsDraggingKeystrokeBody] = useState(false);
  const [isDraggingPointerStart, setIsDraggingPointerStart] = useState(false);
  const [isDraggingPointerEnd, setIsDraggingPointerEnd] = useState(false);
  const [isDraggingPointerBody, setIsDraggingPointerBody] = useState(false);
  const [isDraggingWebcamStart, setIsDraggingWebcamStart] = useState(false);
  const [isDraggingWebcamEnd, setIsDraggingWebcamEnd] = useState(false);
  const [isDraggingWebcamBody, setIsDraggingWebcamBody] = useState(false);
  const [draggingTextId, setDraggingTextId] = useState<string | null>(null);
  const [draggingKeystrokeId, setDraggingKeystrokeId] = useState<string | null>(null);
  const [draggingPointerId, setDraggingPointerId] = useState<string | null>(null);
  const [draggingWebcamId, setDraggingWebcamId] = useState<string | null>(null);
  const textDragOffsetRef = useRef(0);
  const keystrokeDragOffsetRef = useRef(0);
  const pointerDragOffsetRef = useRef(0);
  const webcamDragOffsetRef = useRef(0);
  const keystrokeDragOriginals = useRef<CursorVisibilitySegment[] | null>(null);
  const pointerDragOriginals = useRef<CursorVisibilitySegment[] | null>(null);
  const webcamDragOriginals = useRef<CursorVisibilitySegment[] | null>(null);
  const keystrokeDragDidMove = useRef(false);
  const pointerDragDidMove = useRef(false);
  const webcamDragDidMove = useRef(false);

  // Text drag
  const handleTextDragStart = useCallback((id: string, type: 'start' | 'end' | 'body', offset?: number) => {
    beginBatch();
    setEditingTextId(id);
    setEditingKeystrokeId?.(null);
    setActivePanel('text');
    setDraggingTextId(id);
    if (type === 'start') setIsDraggingTextStart(true);
    else if (type === 'end') setIsDraggingTextEnd(true);
    else if (type === 'body') {
      setIsDraggingTextBody(true);
      if (offset !== undefined) textDragOffsetRef.current = offset;
    }
  }, [beginBatch, setEditingTextId, setEditingKeystrokeId, setActivePanel]);

  const handleTextDrag = useCallback((clientX: number) => {
    if ((!isDraggingTextStart && !isDraggingTextEnd && !isDraggingTextBody) || !draggingTextId || !segment) return;
    const newTime = getTimeFromClientX(clientX);
    if (newTime === null) return;

    setSegment({
      ...segment,
      textSegments: segment.textSegments.map(text => {
        if (text.id !== draggingTextId) return text;
        if (isDraggingTextStart) {
          return { ...text, startTime: Math.min(Math.max(0, newTime), text.endTime - 0.1) };
        } else if (isDraggingTextEnd) {
          return { ...text, endTime: Math.max(Math.min(duration, newTime), text.startTime + 0.1) };
        } else if (isDraggingTextBody) {
          const dur = text.endTime - text.startTime;
          let newStart = newTime - textDragOffsetRef.current;
          if (newStart < 0) newStart = 0;
          if (newStart + dur > duration) newStart = duration - dur;
          return { ...text, startTime: newStart, endTime: newStart + dur };
        }
        return text;
      }),
    });
  }, [isDraggingTextStart, isDraggingTextEnd, isDraggingTextBody, draggingTextId, segment, getTimeFromClientX, setSegment, duration]);

  // Keystroke drag
  const handleKeystrokeDragStart = useCallback((id: string, type: 'start' | 'end' | 'body', offset?: number) => {
    if (!segment || (segment.keystrokeMode ?? 'off') === 'off') return;
    beginBatch();
    keystrokeDragDidMove.current = false;
    keystrokeDragOriginals.current = clampVisibilitySegmentsToDuration(
      getKeystrokeVisibilitySegmentsForMode(segment),
      duration
    ).map((seg) => ({ ...seg }));
    setDraggingKeystrokeId(id);
    setEditingKeystrokeId?.(id);
    if (type === 'start') setIsDraggingKeystrokeStart(true);
    else if (type === 'end') setIsDraggingKeystrokeEnd(true);
    else if (type === 'body') {
      setIsDraggingKeystrokeBody(true);
      if (offset !== undefined) keystrokeDragOffsetRef.current = offset;
    }
  }, [beginBatch, segment, setEditingKeystrokeId, duration]);

  const handleKeystrokeDrag = useCallback((clientX: number) => {
    if (
      (!isDraggingKeystrokeStart && !isDraggingKeystrokeEnd && !isDraggingKeystrokeBody)
      || !draggingKeystrokeId
      || !segment
    ) {
      return;
    }
    const originals = keystrokeDragOriginals.current;
    if (!originals) return;
    const newTime = getTimeFromClientX(clientX);
    if (newTime === null) return;
    keystrokeDragDidMove.current = true;

    const modified = originals.map((seg) => {
      if (seg.id !== draggingKeystrokeId) return { ...seg };
      if (isDraggingKeystrokeStart) {
        return { ...seg, startTime: Math.min(Math.max(0, newTime), seg.endTime - 0.1) };
      }
      if (isDraggingKeystrokeEnd) {
        return { ...seg, endTime: Math.max(Math.min(duration, newTime), seg.startTime + 0.1) };
      }
      if (isDraggingKeystrokeBody) {
        const segDuration = seg.endTime - seg.startTime;
        let nextStart = newTime - keystrokeDragOffsetRef.current;
        if (nextStart < 0) nextStart = 0;
        if (nextStart + segDuration > duration) nextStart = duration - segDuration;
        return { ...seg, startTime: nextStart, endTime: nextStart + segDuration };
      }
      return { ...seg };
    });

    setSegment(withKeystrokeVisibilitySegmentsForMode(
      segment,
      clampVisibilitySegmentsToDuration(mergePointerSegments(modified), duration)
    ));
  }, [
    isDraggingKeystrokeStart,
    isDraggingKeystrokeEnd,
    isDraggingKeystrokeBody,
    draggingKeystrokeId,
    segment,
    getTimeFromClientX,
    setSegment,
    duration,
  ]);

  // Pointer drag
  const handlePointerDragStart = useCallback((id: string, type: 'start' | 'end' | 'body', offset?: number) => {
    beginBatch();
    setEditingKeystrokeId?.(null);
    pointerDragDidMove.current = false;
    // Snapshot originals so merge/unmerge is reversible during drag
    pointerDragOriginals.current = segment?.cursorVisibilitySegments
      ? clampVisibilitySegmentsToDuration(segment.cursorVisibilitySegments, duration).map(s => ({ ...s }))
      : null;
    setDraggingPointerId(id);
    if (type === 'start') setIsDraggingPointerStart(true);
    else if (type === 'end') setIsDraggingPointerEnd(true);
    else if (type === 'body') {
      setIsDraggingPointerBody(true);
      if (offset !== undefined) pointerDragOffsetRef.current = offset;
    }
  }, [beginBatch, segment, setEditingKeystrokeId]);

  const handlePointerDrag = useCallback((clientX: number) => {
    if ((!isDraggingPointerStart && !isDraggingPointerEnd && !isDraggingPointerBody) || !draggingPointerId || !segment) return;
    const originals = pointerDragOriginals.current;
    if (!originals) return;
    const newTime = getTimeFromClientX(clientX);
    if (newTime === null) return;
    pointerDragDidMove.current = true;

    // Recompute from originals so merge/unmerge is reversible during drag
    const modified = originals.map(seg => {
      if (seg.id !== draggingPointerId) return { ...seg };
      if (isDraggingPointerStart) {
        return { ...seg, startTime: Math.min(Math.max(0, newTime), seg.endTime - 0.1) };
      } else if (isDraggingPointerEnd) {
        return { ...seg, endTime: Math.max(Math.min(duration, newTime), seg.startTime + 0.1) };
      } else if (isDraggingPointerBody) {
        const dur = seg.endTime - seg.startTime;
        let newStart = newTime - pointerDragOffsetRef.current;
        if (newStart < 0) newStart = 0;
        if (newStart + dur > duration) newStart = duration - dur;
        return { ...seg, startTime: newStart, endTime: newStart + dur };
      }
      return { ...seg };
    });

    setSegment({
      ...segment,
      cursorVisibilitySegments: clampVisibilitySegmentsToDuration(mergePointerSegments(modified), duration),
    });
  }, [isDraggingPointerStart, isDraggingPointerEnd, isDraggingPointerBody, draggingPointerId, segment, getTimeFromClientX, setSegment, duration]);

  // Pointer click -> split segment at click time
  const handlePointerClick = useCallback((id: string, splitTime: number) => {
    if (isDraggingPointerStart || isDraggingPointerEnd || isDraggingPointerBody) return;
    // Suppress click that fires after a drag (mousedown->move->mouseup->click)
    if (pointerDragDidMove.current) { pointerDragDidMove.current = false; return; }
    if (!segment?.cursorVisibilitySegments) return;

    const seg = segment.cursorVisibilitySegments.find(s => s.id === id);
    if (!seg) return;

    const SPLIT_GAP = 0.3;
    const half = SPLIT_GAP / 2;
    const leftEnd = splitTime - half;
    const rightStart = splitTime + half;

    // Don't split if either resulting piece would be too small
    if (leftEnd - seg.startTime < 0.15 || seg.endTime - rightStart < 0.15) return;

    beginBatch();
    const left: CursorVisibilitySegment = { id: seg.id, startTime: seg.startTime, endTime: leftEnd };
    const right: CursorVisibilitySegment = { id: crypto.randomUUID(), startTime: rightStart, endTime: seg.endTime };

    setSegment({
      ...segment,
      cursorVisibilitySegments: clampVisibilitySegmentsToDuration(segment.cursorVisibilitySegments, duration)
        .filter(s => s.id !== id)
        .concat([left, right])
        .sort((a, b) => a.startTime - b.startTime),
    });
    setEditingPointerId?.(null);
    commitBatch();
  }, [isDraggingPointerStart, isDraggingPointerEnd, isDraggingPointerBody, segment, setSegment, setEditingPointerId, beginBatch, commitBatch, duration]);

  // Webcam drag
  const handleWebcamDragStart = useCallback((id: string, type: 'start' | 'end' | 'body', offset?: number) => {
    beginBatch();
    setEditingKeystrokeId?.(null);
    webcamDragDidMove.current = false;
    webcamDragOriginals.current = segment?.webcamVisibilitySegments
      ? clampVisibilitySegmentsToDuration(segment.webcamVisibilitySegments, duration).map((seg) => ({ ...seg }))
      : null;
    setDraggingWebcamId(id);
    if (type === 'start') setIsDraggingWebcamStart(true);
    else if (type === 'end') setIsDraggingWebcamEnd(true);
    else if (type === 'body') {
      setIsDraggingWebcamBody(true);
      if (offset !== undefined) webcamDragOffsetRef.current = offset;
    }
  }, [beginBatch, segment, setEditingKeystrokeId, duration]);

  const handleWebcamDrag = useCallback((clientX: number) => {
    if ((!isDraggingWebcamStart && !isDraggingWebcamEnd && !isDraggingWebcamBody) || !draggingWebcamId || !segment) return;
    const originals = webcamDragOriginals.current;
    if (!originals) return;
    const newTime = getTimeFromClientX(clientX);
    if (newTime === null) return;
    webcamDragDidMove.current = true;

    const modified = originals.map((seg) => {
      if (seg.id !== draggingWebcamId) return { ...seg };
      if (isDraggingWebcamStart) {
        return { ...seg, startTime: Math.min(Math.max(0, newTime), seg.endTime - 0.1) };
      }
      if (isDraggingWebcamEnd) {
        return { ...seg, endTime: Math.max(Math.min(duration, newTime), seg.startTime + 0.1) };
      }
      if (isDraggingWebcamBody) {
        const segDuration = seg.endTime - seg.startTime;
        let nextStart = newTime - webcamDragOffsetRef.current;
        if (nextStart < 0) nextStart = 0;
        if (nextStart + segDuration > duration) nextStart = duration - segDuration;
        return { ...seg, startTime: nextStart, endTime: nextStart + segDuration };
      }
      return { ...seg };
    });

    setSegment({
      ...segment,
      webcamVisibilitySegments: clampVisibilitySegmentsToDuration(mergePointerSegments(modified), duration),
    });
  }, [isDraggingWebcamStart, isDraggingWebcamEnd, isDraggingWebcamBody, draggingWebcamId, segment, getTimeFromClientX, setSegment, duration]);

  const handleWebcamClick = useCallback((id: string, splitTime: number) => {
    if (isDraggingWebcamStart || isDraggingWebcamEnd || isDraggingWebcamBody) return;
    if (webcamDragDidMove.current) { webcamDragDidMove.current = false; return; }
    if (!segment?.webcamVisibilitySegments) return;

    const seg = segment.webcamVisibilitySegments.find((range) => range.id === id);
    if (!seg) return;

    const SPLIT_GAP = 0.3;
    const half = SPLIT_GAP / 2;
    const leftEnd = splitTime - half;
    const rightStart = splitTime + half;

    if (leftEnd - seg.startTime < 0.15 || seg.endTime - rightStart < 0.15) return;

    beginBatch();
    const left: CursorVisibilitySegment = { id: seg.id, startTime: seg.startTime, endTime: leftEnd };
    const right: CursorVisibilitySegment = { id: crypto.randomUUID(), startTime: rightStart, endTime: seg.endTime };

    setSegment({
      ...segment,
      webcamVisibilitySegments: clampVisibilitySegmentsToDuration(segment.webcamVisibilitySegments, duration)
        .filter((range) => range.id !== id)
        .concat([left, right])
        .sort((a, b) => a.startTime - b.startTime),
    });
    commitBatch();
  }, [isDraggingWebcamStart, isDraggingWebcamEnd, isDraggingWebcamBody, segment, setSegment, beginBatch, commitBatch, duration]);

  // Text click (select)
  const handleTextClick = useCallback((id: string) => {
    if (!isDraggingTextStart && !isDraggingTextEnd) {
      setEditingTextId(id);
      setEditingKeystrokeId?.(null);
      setActivePanel('text');
    }
  }, [isDraggingTextStart, isDraggingTextEnd, setEditingTextId, setEditingKeystrokeId, setActivePanel]);

  const handleKeystrokeClick = useCallback((id: string, splitTime: number) => {
    if (isDraggingKeystrokeStart || isDraggingKeystrokeEnd || isDraggingKeystrokeBody) return;
    if (keystrokeDragDidMove.current) {
      keystrokeDragDidMove.current = false;
      return;
    }
    if (!segment || (segment.keystrokeMode ?? 'off') === 'off') return;

    const segments = clampVisibilitySegmentsToDuration(
      getKeystrokeVisibilitySegmentsForMode(segment),
      duration
    );
    const seg = segments.find((s) => s.id === id);
    if (!seg) return;

    const SPLIT_GAP = 0.3;
    const half = SPLIT_GAP / 2;
    const leftEnd = splitTime - half;
    const rightStart = splitTime + half;

    if (leftEnd - seg.startTime < 0.15 || seg.endTime - rightStart < 0.15) return;

    beginBatch();
    const left: CursorVisibilitySegment = { id: seg.id, startTime: seg.startTime, endTime: leftEnd };
    const right: CursorVisibilitySegment = { id: crypto.randomUUID(), startTime: rightStart, endTime: seg.endTime };

    setSegment(withKeystrokeVisibilitySegmentsForMode(
      segment,
      clampVisibilitySegmentsToDuration(segments
        .filter((s) => s.id !== id)
        .concat([left, right])
        .sort((a, b) => a.startTime - b.startTime), duration)
    ));
    setEditingKeystrokeId?.(null);
    commitBatch();
  }, [isDraggingKeystrokeStart, isDraggingKeystrokeEnd, isDraggingKeystrokeBody, segment, setSegment, setEditingKeystrokeId, beginBatch, commitBatch, duration]);

  const resetTrackDragState = useCallback(() => {
    setIsDraggingTextStart(false);
    setIsDraggingTextEnd(false);
    setIsDraggingTextBody(false);
    setIsDraggingKeystrokeStart(false);
    setIsDraggingKeystrokeEnd(false);
    setIsDraggingKeystrokeBody(false);
    setIsDraggingPointerStart(false);
    setIsDraggingPointerEnd(false);
    setIsDraggingPointerBody(false);
    setIsDraggingWebcamStart(false);
    setIsDraggingWebcamEnd(false);
    setIsDraggingWebcamBody(false);
    setDraggingTextId(null);
    setDraggingKeystrokeId(null);
    setDraggingPointerId(null);
    setDraggingWebcamId(null);
    keystrokeDragOriginals.current = null;
    pointerDragOriginals.current = null;
    webcamDragOriginals.current = null;
  }, []);

  return {
    // State
    isDraggingTextStart,
    isDraggingTextEnd,
    isDraggingTextBody,
    isDraggingKeystrokeStart,
    isDraggingKeystrokeEnd,
    isDraggingKeystrokeBody,
    isDraggingPointerStart,
    isDraggingPointerEnd,
    isDraggingPointerBody,
    isDraggingWebcamStart,
    isDraggingWebcamEnd,
    isDraggingWebcamBody,
    draggingTextId,
    draggingKeystrokeId,
    draggingPointerId,
    draggingWebcamId,
    // Handlers
    handleTextDragStart,
    handleTextDrag,
    handleTextClick,
    handleKeystrokeDragStart,
    handleKeystrokeDrag,
    handleKeystrokeClick,
    handlePointerDragStart,
    handlePointerDrag,
    handlePointerClick,
    handleWebcamDragStart,
    handleWebcamDrag,
    handleWebcamClick,
    resetTrackDragState,
  };
}
