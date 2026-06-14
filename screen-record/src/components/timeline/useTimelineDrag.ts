import { useState, useCallback, useRef, useEffect } from 'react';
import { TrimSegment, ZoomBlock } from '@/types/video';
import { getTrimBounds, getTrimSegments, mergeTrimSegments } from '@/lib/trimSegments';
import { TimelineDragState, UseTimelineDragOptions, ZOOM_KEYFRAME_UNTOUCHABLE_GAP_SEC } from './useTimelineDragTypes';
import { splitVisibilitySegmentAtTime } from './trackDragUtils';
import { useTrackSegmentDrag } from './useTrackSegmentDrag';

export type { TimelineDragState } from './useTimelineDragTypes';

const ZOOM_BLOCK_DRAG_COMMIT_INTERVAL_MS = 32;

export function useTimelineDrag({
  duration,
  segment,
  timelineRef,
  videoRef,
  setCurrentTime,
  setSegment,
  setEditingKeyframeId,
  setEditingTextId,
  setEditingSubtitleId,
  setEditingKeystrokeId,
  setEditingPointerId,
  setActivePanel,
  selectedTextIds,
  selectedSubtitleIds,
  onSeek,
  onSeekEnd,
  onClearTimelineFocus,
  beginBatch,
  commitBatch,
}: UseTimelineDragOptions) {
  const [isDraggingTrimStart, setIsDraggingTrimStart] = useState(false);
  const [isDraggingTrimEnd, setIsDraggingTrimEnd] = useState(false);
  const [isDraggingZoom, setIsDraggingZoom] = useState(false);
  const [isDraggingSeek, setIsDraggingSeek] = useState(false);
  const [draggingZoomIdx, setDraggingZoomIdx] = useState<number | null>(null);
  const draggingZoomIdxRef = useRef<number | null>(null);
  const draggingZoomTokenRef = useRef<string | null>(null);
  const zoomDragTokenMapRef = useRef<WeakMap<ZoomBlock, string>>(new WeakMap());
  const segmentRef = useRef(segment);
  segmentRef.current = segment;
  const zoomDragDraftBlocksRef = useRef<ZoomBlock[] | null>(null);
  const pendingZoomDragBlocksRef = useRef<ZoomBlock[] | null>(null);
  const zoomDragUpdateFrameRef = useRef<number | null>(null);
  const lastZoomDragUpdateAtRef = useRef(0);
  const trimDraggingIdRef = useRef<string | null>(null);
  const trimDragOriginalsRef = useRef<TrimSegment[] | null>(null);

  const flushPendingZoomDragUpdate = useCallback(() => {
    const pendingBlocks = pendingZoomDragBlocksRef.current;
    pendingZoomDragBlocksRef.current = null;
    if (zoomDragUpdateFrameRef.current !== null) {
      cancelAnimationFrame(zoomDragUpdateFrameRef.current);
      zoomDragUpdateFrameRef.current = null;
    }
    if (!pendingBlocks || !segmentRef.current) return;
    lastZoomDragUpdateAtRef.current = performance.now();
    const nextSegment = { ...segmentRef.current, zoomBlocks: pendingBlocks };
    segmentRef.current = nextSegment;
    setSegment(nextSegment);
  }, [setSegment]);

  const scheduleZoomDragUpdate = useCallback((nextBlocks: ZoomBlock[]) => {
    pendingZoomDragBlocksRef.current = nextBlocks;
    if (zoomDragUpdateFrameRef.current !== null) return;

    const pump = () => {
      const now = performance.now();
      if (now - lastZoomDragUpdateAtRef.current < ZOOM_BLOCK_DRAG_COMMIT_INTERVAL_MS) {
        zoomDragUpdateFrameRef.current = requestAnimationFrame(pump);
        return;
      }

      zoomDragUpdateFrameRef.current = null;
      const pendingBlocks = pendingZoomDragBlocksRef.current;
      pendingZoomDragBlocksRef.current = null;
      if (!pendingBlocks || !segmentRef.current) return;
      lastZoomDragUpdateAtRef.current = now;
      const nextSegment = { ...segmentRef.current, zoomBlocks: pendingBlocks };
      segmentRef.current = nextSegment;
      setSegment(nextSegment);
    };

    zoomDragUpdateFrameRef.current = requestAnimationFrame(pump);
  }, [setSegment]);

  useEffect(() => () => {
    if (zoomDragUpdateFrameRef.current !== null) {
      cancelAnimationFrame(zoomDragUpdateFrameRef.current);
      zoomDragUpdateFrameRef.current = null;
    }
    pendingZoomDragBlocksRef.current = null;
  }, []);

  const getTimeFromClientX = useCallback((clientX: number): number | null => {
    const timeline = timelineRef.current;
    if (!timeline) return null;
    const rect = timeline.getBoundingClientRect();
    const x = clientX - rect.left;
    return Math.max(0, Math.min((x / rect.width) * duration, duration));
  }, [timelineRef, duration]);

  // Track segment drag sub-hook (text, subtitles, keystroke, pointer, webcam)
  const trackDrag = useTrackSegmentDrag({
    duration,
    segment,
    setSegment,
    setEditingTextId,
    setEditingSubtitleId,
    setEditingKeystrokeId,
    setEditingPointerId,
    setActivePanel,
    selectedTextIds,
    selectedSubtitleIds,
    getTimeFromClientX,
    beginBatch,
    commitBatch,
  });

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

  // Zoom block body drag — moves the whole block in time, preserving its width.
  const handleZoomDragStart = useCallback((index: number) => {
    beginBatch();
    setIsDraggingZoom(true);
    setDraggingZoomIdx(index);
    draggingZoomIdxRef.current = index;
    const draggedBlock = segment?.zoomBlocks?.[index];
    zoomDragDraftBlocksRef.current = segment?.zoomBlocks ?? [];
    if (draggedBlock) {
      const token = crypto.randomUUID();
      zoomDragTokenMapRef.current.set(draggedBlock, token);
      draggingZoomTokenRef.current = token;
    } else {
      draggingZoomTokenRef.current = null;
    }
    setEditingKeyframeId(index);
    setEditingKeystrokeId?.(null);
    setActivePanel('zoom');
  }, [segment, setEditingKeyframeId, setEditingKeystrokeId, setActivePanel, beginBatch]);

  const handleZoomDrag = useCallback((clientX: number) => {
    if (!isDraggingZoom || draggingZoomIdxRef.current === null || !segment) return;
    const blocks = zoomDragDraftBlocksRef.current ?? segment.zoomBlocks ?? [];
    const newTime = getTimeFromClientX(clientX);
    if (newTime === null) return;

    const dragToken = draggingZoomTokenRef.current;
    let currentIdx = dragToken
      ? blocks.findIndex((b) => zoomDragTokenMapRef.current.get(b) === dragToken)
      : -1;
    if (currentIdx < 0 && draggingZoomIdxRef.current !== null) currentIdx = draggingZoomIdxRef.current;
    if (currentIdx < 0 || currentIdx >= blocks.length) return;

    const block = blocks[currentIdx];
    const width = block.endTime - block.startTime;
    const prev = currentIdx > 0 ? blocks[currentIdx - 1] : null;
    const next = currentIdx < blocks.length - 1 ? blocks[currentIdx + 1] : null;

    // Move so the block center follows the cursor, clamped to keep a sliver gap.
    const lower = prev ? prev.endTime + ZOOM_KEYFRAME_UNTOUCHABLE_GAP_SEC : 0;
    const upper = (next ? next.startTime - ZOOM_KEYFRAME_UNTOUCHABLE_GAP_SEC : duration) - width;
    if (upper < lower) return; // no room to move
    const newStart = Math.max(lower, Math.min(upper, newTime - width / 2));
    const newEnd = newStart + width;

    const movedBlock = { ...block, startTime: newStart, endTime: newEnd };
    if (dragToken) zoomDragTokenMapRef.current.set(movedBlock, dragToken);
    const nextBlocks = blocks
      .map((b, i) => (i === currentIdx ? movedBlock : b))
      .sort((a, b) => a.startTime - b.startTime);
    const newIdx = nextBlocks.indexOf(movedBlock);
    zoomDragDraftBlocksRef.current = nextBlocks;

    draggingZoomIdxRef.current = newIdx;
    if (newIdx !== draggingZoomIdx) {
      setDraggingZoomIdx(newIdx);
      setEditingKeyframeId(newIdx);
    }

    scheduleZoomDragUpdate(nextBlocks);

    // Seek to the center of the solid hold range (between the eased ramps),
    // matching where the badge sits and where a click lands.
    let ei = Math.max(0, movedBlock.easeIn);
    let eo = Math.max(0, movedBlock.easeOut);
    if (ei + eo > width && width > 0) {
      const s = width / (ei + eo);
      ei *= s;
      eo *= s;
    }
    const center = newStart + ei + (width - ei - eo) / 2;
    if (onSeek) {
      onSeek(center);
    } else if (videoRef.current) {
      videoRef.current.currentTime = center;
      setCurrentTime(center);
    }
  }, [isDraggingZoom, draggingZoomIdx, segment, getTimeFromClientX, scheduleZoomDragUpdate, onSeek, videoRef, setCurrentTime, duration, setEditingKeyframeId]);

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
    const nextSegs = splitVisibilitySegmentAtTime(trimSegments, id, splitTime);
    if (!nextSegs) return;

    beginBatch();
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

  // Keyframe click
  const handleKeyframeClick = useCallback((time: number, index: number) => {
    if (onSeek) {
      onSeek(time);
    } else if (videoRef.current) {
      videoRef.current.currentTime = time;
      setCurrentTime(time);
    }
    setEditingKeyframeId(index);
    setEditingKeystrokeId?.(null);
    setActivePanel('zoom');
  }, [onSeek, videoRef, setCurrentTime, setEditingKeyframeId, setEditingKeystrokeId, setActivePanel]);

  // Unified mouse handlers for TimelineArea
  const handleMouseDown = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    if (isDraggingTrimStart || isDraggingTrimEnd || trackDrag.isDraggingTextStart || trackDrag.isDraggingTextEnd || trackDrag.isDraggingTextBody || trackDrag.isDraggingSubtitleStart || trackDrag.isDraggingSubtitleEnd || trackDrag.isDraggingSubtitleBody || trackDrag.isDraggingKeystrokeStart || trackDrag.isDraggingKeystrokeEnd || trackDrag.isDraggingKeystrokeBody || trackDrag.isDraggingPointerStart || trackDrag.isDraggingPointerEnd || trackDrag.isDraggingPointerBody || trackDrag.isDraggingWebcamStart || trackDrag.isDraggingWebcamEnd || trackDrag.isDraggingWebcamBody || isDraggingZoom) return;
    setIsDraggingSeek(true);
    setEditingTextId(null);
    setEditingSubtitleId?.(null);
    setEditingKeystrokeId?.(null);
    setEditingPointerId?.(null);
    onClearTimelineFocus?.();
    handleSeek(e.clientX);
  }, [isDraggingTrimStart, isDraggingTrimEnd, trackDrag.isDraggingTextStart, trackDrag.isDraggingTextEnd, trackDrag.isDraggingTextBody, trackDrag.isDraggingSubtitleStart, trackDrag.isDraggingSubtitleEnd, trackDrag.isDraggingSubtitleBody, trackDrag.isDraggingKeystrokeStart, trackDrag.isDraggingKeystrokeEnd, trackDrag.isDraggingKeystrokeBody, trackDrag.isDraggingPointerStart, trackDrag.isDraggingPointerEnd, trackDrag.isDraggingPointerBody, trackDrag.isDraggingWebcamStart, trackDrag.isDraggingWebcamEnd, trackDrag.isDraggingWebcamBody, isDraggingZoom, setEditingTextId, setEditingSubtitleId, setEditingKeystrokeId, setEditingPointerId, onClearTimelineFocus, handleSeek]);

  const handleMouseMove = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    handleTrimDrag(e.clientX);
    trackDrag.handleTextDrag(e.clientX);
    trackDrag.handleSubtitleDrag(e.clientX);
    trackDrag.handleKeystrokeDrag(e.clientX);
    trackDrag.handlePointerDrag(e.clientX);
    trackDrag.handleWebcamDrag(e.clientX);
    handleZoomDrag(e.clientX);
    if (isDraggingSeek) handleSeek(e.clientX);
  }, [handleTrimDrag, trackDrag.handleTextDrag, trackDrag.handleSubtitleDrag, trackDrag.handleKeystrokeDrag, trackDrag.handlePointerDrag, trackDrag.handleWebcamDrag, handleZoomDrag, isDraggingSeek, handleSeek]);

  const handleMouseUp = useCallback(() => {
    // Commit batch if any drag operation was active (not seek -- seek doesn't modify segment)
    if (isDraggingTrimStart || isDraggingTrimEnd || trackDrag.isDraggingTextStart || trackDrag.isDraggingTextEnd || trackDrag.isDraggingTextBody || trackDrag.isDraggingSubtitleStart || trackDrag.isDraggingSubtitleEnd || trackDrag.isDraggingSubtitleBody || trackDrag.isDraggingKeystrokeStart || trackDrag.isDraggingKeystrokeEnd || trackDrag.isDraggingKeystrokeBody || trackDrag.isDraggingPointerStart || trackDrag.isDraggingPointerEnd || trackDrag.isDraggingPointerBody || trackDrag.isDraggingWebcamStart || trackDrag.isDraggingWebcamEnd || trackDrag.isDraggingWebcamBody || isDraggingZoom) {
      if (isDraggingZoom) flushPendingZoomDragUpdate();
      commitBatch();
    }
    // Flush any pending throttled seek so the final position is applied
    if (isDraggingSeek) onSeekEnd?.();
    setIsDraggingTrimStart(false);
    setIsDraggingTrimEnd(false);
    trimDragOriginalsRef.current = null;
    trimDraggingIdRef.current = null;
    trackDrag.resetTrackDragState();
    setIsDraggingZoom(false);
    setDraggingZoomIdx(null);
    draggingZoomIdxRef.current = null;
    draggingZoomTokenRef.current = null;
    zoomDragDraftBlocksRef.current = null;
    setIsDraggingSeek(false);
  }, [isDraggingTrimStart, isDraggingTrimEnd, trackDrag.isDraggingTextStart, trackDrag.isDraggingTextEnd, trackDrag.isDraggingTextBody, trackDrag.isDraggingSubtitleStart, trackDrag.isDraggingSubtitleEnd, trackDrag.isDraggingSubtitleBody, trackDrag.isDraggingKeystrokeStart, trackDrag.isDraggingKeystrokeEnd, trackDrag.isDraggingKeystrokeBody, trackDrag.isDraggingPointerStart, trackDrag.isDraggingPointerEnd, trackDrag.isDraggingPointerBody, trackDrag.isDraggingWebcamStart, trackDrag.isDraggingWebcamEnd, trackDrag.isDraggingWebcamBody, isDraggingZoom, isDraggingSeek, flushPendingZoomDragUpdate, commitBatch, onSeekEnd, trackDrag.resetTrackDragState]);

  // Attach window-level listeners during any drag so cursor can leave the timeline
  useEffect(() => {
    const anyDragging = isDraggingTrimStart || isDraggingTrimEnd || trackDrag.isDraggingTextStart || trackDrag.isDraggingTextEnd || trackDrag.isDraggingTextBody || trackDrag.isDraggingSubtitleStart || trackDrag.isDraggingSubtitleEnd || trackDrag.isDraggingSubtitleBody || trackDrag.isDraggingKeystrokeStart || trackDrag.isDraggingKeystrokeEnd || trackDrag.isDraggingKeystrokeBody || trackDrag.isDraggingPointerStart || trackDrag.isDraggingPointerEnd || trackDrag.isDraggingPointerBody || trackDrag.isDraggingWebcamStart || trackDrag.isDraggingWebcamEnd || trackDrag.isDraggingWebcamBody || isDraggingZoom || isDraggingSeek;
    if (!anyDragging) return;

    const onMove = (e: PointerEvent) => {
      handleTrimDrag(e.clientX);
      trackDrag.handleTextDrag(e.clientX);
      trackDrag.handleSubtitleDrag(e.clientX);
      trackDrag.handleKeystrokeDrag(e.clientX);
      trackDrag.handlePointerDrag(e.clientX);
      trackDrag.handleWebcamDrag(e.clientX);
      handleZoomDrag(e.clientX);
      if (isDraggingSeek) handleSeek(e.clientX);
    };
    const onUp = () => handleMouseUp();

    window.addEventListener('pointermove', onMove);
    window.addEventListener('pointerup', onUp);
    window.addEventListener('pointercancel', onUp);
    return () => {
      window.removeEventListener('pointermove', onMove);
      window.removeEventListener('pointerup', onUp);
      window.removeEventListener('pointercancel', onUp);
    };
  }, [isDraggingTrimStart, isDraggingTrimEnd, trackDrag.isDraggingTextStart, trackDrag.isDraggingTextEnd, trackDrag.isDraggingTextBody, trackDrag.isDraggingSubtitleStart, trackDrag.isDraggingSubtitleEnd, trackDrag.isDraggingSubtitleBody, trackDrag.isDraggingKeystrokeStart, trackDrag.isDraggingKeystrokeEnd, trackDrag.isDraggingKeystrokeBody, trackDrag.isDraggingPointerStart, trackDrag.isDraggingPointerEnd, trackDrag.isDraggingPointerBody, trackDrag.isDraggingWebcamStart, trackDrag.isDraggingWebcamEnd, trackDrag.isDraggingWebcamBody, isDraggingZoom, isDraggingSeek, handleTrimDrag, trackDrag.handleTextDrag, trackDrag.handleSubtitleDrag, trackDrag.handleKeystrokeDrag, trackDrag.handlePointerDrag, trackDrag.handleWebcamDrag, handleZoomDrag, handleSeek, handleMouseUp]);

  // Enforce drag cursor globally and suppress hover UI on other timeline tracks while dragging.
  useEffect(() => {
    const isEwResize =
      isDraggingTrimStart ||
      isDraggingTrimEnd ||
      trackDrag.isDraggingTextStart ||
      trackDrag.isDraggingTextEnd ||
      trackDrag.isDraggingSubtitleStart ||
      trackDrag.isDraggingSubtitleEnd ||
      trackDrag.isDraggingKeystrokeStart ||
      trackDrag.isDraggingKeystrokeEnd ||
      trackDrag.isDraggingPointerStart ||
      trackDrag.isDraggingPointerEnd ||
      trackDrag.isDraggingWebcamStart ||
      trackDrag.isDraggingWebcamEnd ||
      isDraggingZoom;
    const isMove = trackDrag.isDraggingTextBody || trackDrag.isDraggingSubtitleBody || trackDrag.isDraggingKeystrokeBody || trackDrag.isDraggingPointerBody || trackDrag.isDraggingWebcamBody;

    if (isEwResize) document.body.classList.add('dragging-ew');
    else document.body.classList.remove('dragging-ew');

    if (isMove) document.body.classList.add('dragging-move');
    else document.body.classList.remove('dragging-move');

    if (isDraggingSeek) document.body.classList.add('dragging-seek');
    else document.body.classList.remove('dragging-seek');

    return () => {
      document.body.classList.remove('dragging-ew');
      document.body.classList.remove('dragging-move');
      document.body.classList.remove('dragging-seek');
    };
  }, [
    isDraggingTrimStart,
    isDraggingTrimEnd,
    trackDrag.isDraggingTextStart,
    trackDrag.isDraggingTextEnd,
    trackDrag.isDraggingTextBody,
    trackDrag.isDraggingSubtitleStart,
    trackDrag.isDraggingSubtitleEnd,
    trackDrag.isDraggingSubtitleBody,
    trackDrag.isDraggingKeystrokeStart,
    trackDrag.isDraggingKeystrokeEnd,
    trackDrag.isDraggingKeystrokeBody,
    trackDrag.isDraggingPointerStart,
    trackDrag.isDraggingPointerEnd,
    trackDrag.isDraggingPointerBody,
    trackDrag.isDraggingWebcamStart,
    trackDrag.isDraggingWebcamEnd,
    trackDrag.isDraggingWebcamBody,
    isDraggingZoom,
    isDraggingSeek,
  ]);

  const dragState: TimelineDragState = {
    isDraggingTrimStart,
    isDraggingTrimEnd,
    isDraggingTextStart: trackDrag.isDraggingTextStart,
    isDraggingTextEnd: trackDrag.isDraggingTextEnd,
    isDraggingTextBody: trackDrag.isDraggingTextBody,
    isDraggingSubtitleStart: trackDrag.isDraggingSubtitleStart,
    isDraggingSubtitleEnd: trackDrag.isDraggingSubtitleEnd,
    isDraggingSubtitleBody: trackDrag.isDraggingSubtitleBody,
    isDraggingKeystrokeStart: trackDrag.isDraggingKeystrokeStart,
    isDraggingKeystrokeEnd: trackDrag.isDraggingKeystrokeEnd,
    isDraggingKeystrokeBody: trackDrag.isDraggingKeystrokeBody,
    isDraggingPointerStart: trackDrag.isDraggingPointerStart,
    isDraggingPointerEnd: trackDrag.isDraggingPointerEnd,
    isDraggingPointerBody: trackDrag.isDraggingPointerBody,
    isDraggingWebcamStart: trackDrag.isDraggingWebcamStart,
    isDraggingWebcamEnd: trackDrag.isDraggingWebcamEnd,
    isDraggingWebcamBody: trackDrag.isDraggingWebcamBody,
    isDraggingZoom,
    isDraggingSeek,
    draggingTextId: trackDrag.draggingTextId,
    draggingSubtitleId: trackDrag.draggingSubtitleId,
    draggingKeystrokeId: trackDrag.draggingKeystrokeId,
    draggingPointerId: trackDrag.draggingPointerId,
    draggingWebcamId: trackDrag.draggingWebcamId,
    draggingZoomIdx,
  };

  return {
    dragState,
    handleSeek,
    handleTrimDragStart,
    handleTrimSplit,
    handleTrimAddSegment,
    handleZoomDragStart,
    handleTextDragStart: trackDrag.handleTextDragStart,
    handleTextClick: trackDrag.handleTextClick,
    handleSubtitleDragStart: trackDrag.handleSubtitleDragStart,
    handleSubtitleClick: trackDrag.handleSubtitleClick,
    handleKeystrokeDragStart: trackDrag.handleKeystrokeDragStart,
    handleKeystrokeClick: trackDrag.handleKeystrokeClick,
    handlePointerDragStart: trackDrag.handlePointerDragStart,
    handlePointerClick: trackDrag.handlePointerClick,
    handleWebcamDragStart: trackDrag.handleWebcamDragStart,
    handleWebcamClick: trackDrag.handleWebcamClick,
    handleKeyframeClick,
    handleMouseDown,
    handleMouseMove,
    handleMouseUp,
  };
}
