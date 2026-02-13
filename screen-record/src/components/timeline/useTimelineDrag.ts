import { useState, useCallback, useRef, useEffect } from 'react';
import { VideoSegment, CursorVisibilitySegment, TrimSegment } from '@/types/video';
import { mergePointerSegments } from '@/lib/cursorHiding';
import { getTrimBounds, getTrimSegments, mergeTrimSegments } from '@/lib/trimSegments';

export interface TimelineDragState {
  isDraggingTrimStart: boolean;
  isDraggingTrimEnd: boolean;
  isDraggingTextStart: boolean;
  isDraggingTextEnd: boolean;
  isDraggingTextBody: boolean;
  isDraggingPointerStart: boolean;
  isDraggingPointerEnd: boolean;
  isDraggingPointerBody: boolean;
  isDraggingZoom: boolean;
  isDraggingSeek: boolean;
  draggingTextId: string | null;
  draggingPointerId: string | null;
  draggingZoomIdx: number | null;
}

interface UseTimelineDragOptions {
  duration: number;
  segment: VideoSegment | null;
  timelineRef: React.RefObject<HTMLDivElement>;
  videoRef: React.RefObject<HTMLVideoElement>;
  setCurrentTime: (time: number) => void;
  setSegment: (segment: VideoSegment | null) => void;
  setEditingKeyframeId: (id: number | null) => void;
  setEditingTextId: (id: string | null) => void;
  setEditingPointerId?: (id: string | null) => void;
  setActivePanel: (panel: 'zoom' | 'background' | 'cursor' | 'text') => void;
  onSeek?: (time: number) => void;
  onSeekEnd?: () => void;
  beginBatch: () => void;
  commitBatch: () => void;
}

export function useTimelineDrag({
  duration,
  segment,
  timelineRef,
  videoRef,
  setCurrentTime,
  setSegment,
  setEditingKeyframeId,
  setEditingTextId,
  setEditingPointerId,
  setActivePanel,
  onSeek,
  onSeekEnd,
  beginBatch,
  commitBatch,
}: UseTimelineDragOptions) {
  const [isDraggingTrimStart, setIsDraggingTrimStart] = useState(false);
  const [isDraggingTrimEnd, setIsDraggingTrimEnd] = useState(false);
  const [isDraggingTextStart, setIsDraggingTextStart] = useState(false);
  const [isDraggingTextEnd, setIsDraggingTextEnd] = useState(false);
  const [isDraggingTextBody, setIsDraggingTextBody] = useState(false);
  const [isDraggingPointerStart, setIsDraggingPointerStart] = useState(false);
  const [isDraggingPointerEnd, setIsDraggingPointerEnd] = useState(false);
  const [isDraggingPointerBody, setIsDraggingPointerBody] = useState(false);
  const [isDraggingZoom, setIsDraggingZoom] = useState(false);
  const [isDraggingSeek, setIsDraggingSeek] = useState(false);
  const [draggingTextId, setDraggingTextId] = useState<string | null>(null);
  const [draggingPointerId, setDraggingPointerId] = useState<string | null>(null);
  const [draggingZoomIdx, setDraggingZoomIdx] = useState<number | null>(null);
  const textDragOffsetRef = useRef(0);
  const pointerDragOffsetRef = useRef(0);
  const trimDraggingIdRef = useRef<string | null>(null);
  const trimDragOriginalsRef = useRef<TrimSegment[] | null>(null);
  const pointerDragOriginals = useRef<CursorVisibilitySegment[] | null>(null);
  const pointerDragDidMove = useRef(false);

  const getTimeFromClientX = useCallback((clientX: number): number | null => {
    const timeline = timelineRef.current;
    if (!timeline) return null;
    const rect = timeline.getBoundingClientRect();
    const x = clientX - rect.left;
    return Math.max(0, Math.min((x / rect.width) * duration, duration));
  }, [timelineRef, duration]);

  // Seek
  const handleSeek = useCallback((clientX: number) => {
    const time = getTimeFromClientX(clientX);
    if (time === null || !segment) return;
    if (onSeek) {
      onSeek(time);
    } else if (videoRef.current && Math.abs(videoRef.current.currentTime - time) > 0.05) {
      videoRef.current.currentTime = time;
      setCurrentTime(time);
    }
  }, [getTimeFromClientX, segment, onSeek, videoRef, setCurrentTime]);

  // Zoom keyframe drag
  const handleZoomDragStart = useCallback((index: number) => {
    beginBatch();
    setIsDraggingZoom(true);
    setDraggingZoomIdx(index);
    setEditingKeyframeId(index);
    setActivePanel('zoom');
  }, [setEditingKeyframeId, setActivePanel, beginBatch]);

  const handleZoomDrag = useCallback((clientX: number) => {
    if (!isDraggingZoom || draggingZoomIdx === null || !segment) return;
    const newTime = getTimeFromClientX(clientX);
    if (newTime === null) return;

    const prevKeyframe = draggingZoomIdx > 0 ? segment.zoomKeyframes[draggingZoomIdx - 1] : null;
    const nextKeyframe = draggingZoomIdx < segment.zoomKeyframes.length - 1 ? segment.zoomKeyframes[draggingZoomIdx + 1] : null;

    let finalTime = newTime;
    if (prevKeyframe && finalTime <= prevKeyframe.time + 0.1) finalTime = prevKeyframe.time + 0.1;
    if (nextKeyframe && finalTime >= nextKeyframe.time - 0.1) finalTime = nextKeyframe.time - 0.1;

    setSegment({
      ...segment,
      zoomKeyframes: segment.zoomKeyframes.map((kf, i) =>
        i === draggingZoomIdx ? { ...kf, time: finalTime } : kf
      ),
    });

    if (onSeek) {
      onSeek(finalTime);
    } else if (videoRef.current) {
      videoRef.current.currentTime = finalTime;
      setCurrentTime(finalTime);
    }
  }, [isDraggingZoom, draggingZoomIdx, segment, getTimeFromClientX, setSegment, onSeek, videoRef, setCurrentTime]);

  // Trim drag
  const handleTrimDragStart = useCallback((id: string, type: 'start' | 'end') => {
    beginBatch();
    trimDraggingIdRef.current = id;
    trimDragOriginalsRef.current = segment ? getTrimSegments(segment, duration) : null;
    if (type === 'start') setIsDraggingTrimStart(true);
    else setIsDraggingTrimEnd(true);
  }, [beginBatch, segment, duration]);

  const handleTrimDrag = useCallback((clientX: number) => {
    if (!isDraggingTrimStart && !isDraggingTrimEnd) return;
    if (!segment) return;
    const originals = trimDragOriginalsRef.current;
    const draggingId = trimDraggingIdRef.current;
    if (!originals || !draggingId) return;
    const newTime = getTimeFromClientX(clientX);
    if (newTime === null) return;

    const moved = originals.map(seg => {
      if (seg.id !== draggingId) return { ...seg };
      if (isDraggingTrimStart) {
        return {
          ...seg,
          startTime: Math.min(Math.max(0, newTime), seg.endTime - 0.1),
        };
      }
      if (isDraggingTrimEnd) {
        return {
          ...seg,
          endTime: Math.max(Math.min(duration, newTime), seg.startTime + 0.1),
        };
      }
      return { ...seg };
    });

    const merged = mergeTrimSegments(moved);
    const bounds = getTrimBounds({ ...segment, trimSegments: merged }, duration);
    setSegment({
      ...segment,
      trimSegments: merged,
      trimStart: bounds.trimStart,
      trimEnd: bounds.trimEnd,
    });
    if (videoRef.current) videoRef.current.currentTime = newTime;
  }, [isDraggingTrimStart, isDraggingTrimEnd, segment, getTimeFromClientX, setSegment, videoRef, duration]);

  const handleTrimSplit = useCallback((id: string, splitTime: number) => {
    if (!segment) return;
    const trimSegments = getTrimSegments(segment, duration);
    const seg = trimSegments.find(s => s.id === id);
    if (!seg) return;

    const SPLIT_GAP = 0.3;
    const half = SPLIT_GAP / 2;
    const leftEnd = splitTime - half;
    const rightStart = splitTime + half;

    if (leftEnd - seg.startTime < 0.15 || seg.endTime - rightStart < 0.15) return;

    beginBatch();
    const nextSegs = trimSegments
      .filter(s => s.id !== id)
      .concat([
        { id: seg.id, startTime: seg.startTime, endTime: leftEnd },
        { id: crypto.randomUUID(), startTime: rightStart, endTime: seg.endTime },
      ])
      .sort((a, b) => a.startTime - b.startTime);

    const bounds = getTrimBounds({ ...segment, trimSegments: nextSegs }, duration);
    setSegment({
      ...segment,
      trimSegments: nextSegs,
      trimStart: bounds.trimStart,
      trimEnd: bounds.trimEnd,
    });
    commitBatch();
  }, [segment, duration, beginBatch, setSegment, commitBatch]);

  const handleTrimAddSegment = useCallback((atTime: number) => {
    if (!segment) return;
    const trimSegments = getTrimSegments(segment, duration);
    const sorted = [...trimSegments].sort((a, b) => a.startTime - b.startTime);
    const gaps: Array<{ start: number; end: number }> = [];
    let cursor = 0;
    for (const seg of sorted) {
      if (seg.startTime > cursor) gaps.push({ start: cursor, end: seg.startTime });
      cursor = seg.endTime;
    }
    if (cursor < duration) gaps.push({ start: cursor, end: duration });

    const gap = gaps.find(g => atTime >= g.start && atTime <= g.end);
    if (!gap) return;

    const segDur = 2;
    let startTime = Math.max(gap.start, atTime - segDur / 2);
    let endTime = Math.min(gap.end, startTime + segDur);
    if (endTime - startTime < 0.1) {
      endTime = gap.end;
      startTime = Math.max(gap.start, endTime - 0.1);
    }
    if (endTime - startTime < 0.1) return;

    beginBatch();
    const nextSegs = mergeTrimSegments([
      ...trimSegments,
      { id: crypto.randomUUID(), startTime, endTime },
    ]).sort((a, b) => a.startTime - b.startTime);
    const bounds = getTrimBounds({ ...segment, trimSegments: nextSegs }, duration);
    setSegment({
      ...segment,
      trimSegments: nextSegs,
      trimStart: bounds.trimStart,
      trimEnd: bounds.trimEnd,
    });
    commitBatch();
  }, [segment, duration, beginBatch, setSegment, commitBatch]);

  // Text drag
  const handleTextDragStart = useCallback((id: string, type: 'start' | 'end' | 'body', offset?: number) => {
    beginBatch();
    setDraggingTextId(id);
    if (type === 'start') setIsDraggingTextStart(true);
    else if (type === 'end') setIsDraggingTextEnd(true);
    else if (type === 'body') {
      setIsDraggingTextBody(true);
      if (offset !== undefined) textDragOffsetRef.current = offset;
    }
  }, [beginBatch]);

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

  // Pointer drag
  const handlePointerDragStart = useCallback((id: string, type: 'start' | 'end' | 'body', offset?: number) => {
    beginBatch();
    pointerDragDidMove.current = false;
    // Snapshot originals so merge/unmerge is reversible during drag
    pointerDragOriginals.current = segment?.cursorVisibilitySegments
      ? segment.cursorVisibilitySegments.map(s => ({ ...s }))
      : null;
    setDraggingPointerId(id);
    if (type === 'start') setIsDraggingPointerStart(true);
    else if (type === 'end') setIsDraggingPointerEnd(true);
    else if (type === 'body') {
      setIsDraggingPointerBody(true);
      if (offset !== undefined) pointerDragOffsetRef.current = offset;
    }
  }, [beginBatch, segment]);

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
      cursorVisibilitySegments: mergePointerSegments(modified),
    });
  }, [isDraggingPointerStart, isDraggingPointerEnd, isDraggingPointerBody, draggingPointerId, segment, getTimeFromClientX, setSegment, duration]);

  // Pointer click → split segment at click time
  const handlePointerClick = useCallback((id: string, splitTime: number) => {
    if (isDraggingPointerStart || isDraggingPointerEnd || isDraggingPointerBody) return;
    // Suppress click that fires after a drag (mousedown→move→mouseup→click)
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
      cursorVisibilitySegments: segment.cursorVisibilitySegments
        .filter(s => s.id !== id)
        .concat([left, right])
        .sort((a, b) => a.startTime - b.startTime),
    });
    setEditingPointerId?.(null);
    commitBatch();
  }, [isDraggingPointerStart, isDraggingPointerEnd, isDraggingPointerBody, segment, setSegment, setEditingPointerId, beginBatch, commitBatch]);

  // Text click (select)
  const handleTextClick = useCallback((id: string) => {
    if (!isDraggingTextStart && !isDraggingTextEnd) {
      setEditingTextId(id);
      setActivePanel('text');
    }
  }, [isDraggingTextStart, isDraggingTextEnd, setEditingTextId, setActivePanel]);

  // Keyframe click
  const handleKeyframeClick = useCallback((time: number, index: number) => {
    if (onSeek) {
      onSeek(time);
    } else if (videoRef.current) {
      videoRef.current.currentTime = time;
      setCurrentTime(time);
    }
    setEditingKeyframeId(index);
    setActivePanel('zoom');
  }, [onSeek, videoRef, setCurrentTime, setEditingKeyframeId, setActivePanel]);

  // Unified mouse handlers for TimelineArea
  const handleMouseDown = useCallback((e: React.MouseEvent<HTMLDivElement>) => {
    if (isDraggingTrimStart || isDraggingTrimEnd || isDraggingTextStart || isDraggingTextEnd || isDraggingTextBody || isDraggingPointerStart || isDraggingPointerEnd || isDraggingPointerBody || isDraggingZoom) return;
    setIsDraggingSeek(true);
    setEditingTextId(null);
    setEditingPointerId?.(null);
    handleSeek(e.clientX);
  }, [isDraggingTrimStart, isDraggingTrimEnd, isDraggingTextStart, isDraggingTextEnd, isDraggingTextBody, isDraggingPointerStart, isDraggingPointerEnd, isDraggingPointerBody, isDraggingZoom, setEditingTextId, setEditingPointerId, handleSeek]);

  const handleMouseMove = useCallback((e: React.MouseEvent<HTMLDivElement>) => {
    handleTrimDrag(e.clientX);
    handleTextDrag(e.clientX);
    handlePointerDrag(e.clientX);
    handleZoomDrag(e.clientX);
    if (isDraggingSeek) handleSeek(e.clientX);
  }, [handleTrimDrag, handleTextDrag, handlePointerDrag, handleZoomDrag, isDraggingSeek, handleSeek]);

  const handleMouseUp = useCallback(() => {
    // Commit batch if any drag operation was active (not seek — seek doesn't modify segment)
    if (isDraggingTrimStart || isDraggingTrimEnd || isDraggingTextStart || isDraggingTextEnd || isDraggingTextBody || isDraggingPointerStart || isDraggingPointerEnd || isDraggingPointerBody || isDraggingZoom) {
      commitBatch();
    }
    // Flush any pending throttled seek so the final position is applied
    if (isDraggingSeek) onSeekEnd?.();
    setIsDraggingTrimStart(false);
    setIsDraggingTrimEnd(false);
    trimDragOriginalsRef.current = null;
    trimDraggingIdRef.current = null;
    setIsDraggingTextStart(false);
    setIsDraggingTextEnd(false);
    setIsDraggingTextBody(false);
    setIsDraggingPointerStart(false);
    setIsDraggingPointerEnd(false);
    setIsDraggingPointerBody(false);
    setIsDraggingZoom(false);
    setDraggingZoomIdx(null);
    setDraggingTextId(null);
    setDraggingPointerId(null);
    pointerDragOriginals.current = null;
    setIsDraggingSeek(false);
  }, [isDraggingTrimStart, isDraggingTrimEnd, isDraggingTextStart, isDraggingTextEnd, isDraggingTextBody, isDraggingPointerStart, isDraggingPointerEnd, isDraggingPointerBody, isDraggingZoom, isDraggingSeek, commitBatch, onSeekEnd]);

  // Attach window-level listeners during any drag so cursor can leave the timeline
  useEffect(() => {
    const anyDragging = isDraggingTrimStart || isDraggingTrimEnd || isDraggingTextStart || isDraggingTextEnd || isDraggingTextBody || isDraggingPointerStart || isDraggingPointerEnd || isDraggingPointerBody || isDraggingZoom || isDraggingSeek;
    if (!anyDragging) return;

    const onMove = (e: MouseEvent) => {
      handleTrimDrag(e.clientX);
      handleTextDrag(e.clientX);
      handlePointerDrag(e.clientX);
      handleZoomDrag(e.clientX);
      if (isDraggingSeek) handleSeek(e.clientX);
    };
    const onUp = () => handleMouseUp();

    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
    return () => {
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
    };
  }, [isDraggingTrimStart, isDraggingTrimEnd, isDraggingTextStart, isDraggingTextEnd, isDraggingTextBody, isDraggingPointerStart, isDraggingPointerEnd, isDraggingPointerBody, isDraggingZoom, isDraggingSeek, handleTrimDrag, handleTextDrag, handlePointerDrag, handleZoomDrag, handleSeek, handleMouseUp]);

  const dragState: TimelineDragState = {
    isDraggingTrimStart,
    isDraggingTrimEnd,
    isDraggingTextStart,
    isDraggingTextEnd,
    isDraggingTextBody,
    isDraggingPointerStart,
    isDraggingPointerEnd,
    isDraggingPointerBody,
    isDraggingZoom,
    isDraggingSeek,
    draggingTextId,
    draggingPointerId,
    draggingZoomIdx,
  };

  return {
    dragState,
    handleSeek,
    handleTrimDragStart,
    handleTrimSplit,
    handleTrimAddSegment,
    handleZoomDragStart,
    handleTextDragStart,
    handleTextClick,
    handlePointerDragStart,
    handlePointerClick,
    handleKeyframeClick,
    handleMouseDown,
    handleMouseMove,
    handleMouseUp,
  };
}
