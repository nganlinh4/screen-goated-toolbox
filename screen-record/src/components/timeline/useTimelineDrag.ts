import { useState, useCallback, useRef, useEffect } from 'react';
import { VideoSegment } from '@/types/video';

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

    if (videoRef.current) {
      videoRef.current.currentTime = finalTime;
      setCurrentTime(finalTime);
    }
  }, [isDraggingZoom, draggingZoomIdx, segment, getTimeFromClientX, setSegment, videoRef, setCurrentTime]);

  // Trim drag
  const handleTrimDragStart = useCallback((type: 'start' | 'end') => {
    beginBatch();
    if (type === 'start') setIsDraggingTrimStart(true);
    else setIsDraggingTrimEnd(true);
  }, [beginBatch]);

  const handleTrimDrag = useCallback((clientX: number) => {
    if (!isDraggingTrimStart && !isDraggingTrimEnd) return;
    if (!segment) return;
    const newTime = getTimeFromClientX(clientX);
    if (newTime === null) return;

    if (isDraggingTrimStart) {
      const newTrimStart = Math.min(newTime, segment.trimEnd - 0.1);
      setSegment({ ...segment, trimStart: Math.max(0, newTrimStart) });
      if (videoRef.current) videoRef.current.currentTime = newTime;
    }
    if (isDraggingTrimEnd) {
      const newTrimEnd = Math.max(newTime, segment.trimStart + 0.1);
      setSegment({ ...segment, trimEnd: Math.min(duration, newTrimEnd) });
      if (videoRef.current) videoRef.current.currentTime = newTime;
    }
  }, [isDraggingTrimStart, isDraggingTrimEnd, segment, getTimeFromClientX, setSegment, videoRef, duration]);

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
    setDraggingPointerId(id);
    if (type === 'start') setIsDraggingPointerStart(true);
    else if (type === 'end') setIsDraggingPointerEnd(true);
    else if (type === 'body') {
      setIsDraggingPointerBody(true);
      if (offset !== undefined) pointerDragOffsetRef.current = offset;
    }
  }, [beginBatch]);

  const handlePointerDrag = useCallback((clientX: number) => {
    if ((!isDraggingPointerStart && !isDraggingPointerEnd && !isDraggingPointerBody) || !draggingPointerId || !segment) return;
    const newTime = getTimeFromClientX(clientX);
    if (newTime === null) return;

    setSegment({
      ...segment,
      cursorVisibilitySegments: segment.cursorVisibilitySegments?.map(seg => {
        if (seg.id !== draggingPointerId) return seg;
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
        return seg;
      }),
    });
  }, [isDraggingPointerStart, isDraggingPointerEnd, isDraggingPointerBody, draggingPointerId, segment, getTimeFromClientX, setSegment, duration]);

  // Pointer click (select)
  const handlePointerClick = useCallback((id: string) => {
    if (!isDraggingPointerStart && !isDraggingPointerEnd) {
      setEditingPointerId?.(id);
    }
  }, [isDraggingPointerStart, isDraggingPointerEnd, setEditingPointerId]);

  // Text click (select)
  const handleTextClick = useCallback((id: string) => {
    if (!isDraggingTextStart && !isDraggingTextEnd) {
      setEditingTextId(id);
      setActivePanel('text');
    }
  }, [isDraggingTextStart, isDraggingTextEnd, setEditingTextId, setActivePanel]);

  // Keyframe click
  const handleKeyframeClick = useCallback((time: number, index: number) => {
    if (videoRef.current) {
      videoRef.current.currentTime = time;
      setCurrentTime(time);
      setEditingKeyframeId(index);
      setActivePanel('zoom');
    }
  }, [videoRef, setCurrentTime, setEditingKeyframeId, setActivePanel]);

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
