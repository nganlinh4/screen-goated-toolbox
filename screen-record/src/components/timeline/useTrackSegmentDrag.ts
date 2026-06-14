import { useState, useCallback, useRef } from 'react';
import type { SubtitleSegment } from '@/types/video';
import { clampVisibilitySegmentsToDuration, mergePointerSegments } from '@/lib/cursorHiding';
import { getKeystrokeVisibilitySegmentsForMode, withKeystrokeVisibilitySegmentsForMode } from '@/lib/keystrokeVisibility';
import { updateSubtitleTimingAcrossTracks, updateSubtitleTimingsAcrossTracks } from '@/lib/subtitleTrackMutations';
import { getVisibleSubtitleSegments } from '@/lib/subtitleTracks';
import { clampSegmentEdge, computeGroupDragDelta, snapshotSegmentBounds, type DragOriginalBounds } from './trackDragUtils';
import { useVisibilitySegmentDrag } from './useVisibilitySegmentDrag';
import type { UseTrackSegmentDragOptions } from './useTrackSegmentDragTypes';

export function useTrackSegmentDrag({
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
}: UseTrackSegmentDragOptions) {
  const [isDraggingTextStart, setIsDraggingTextStart] = useState(false);
  const [isDraggingTextEnd, setIsDraggingTextEnd] = useState(false);
  const [isDraggingTextBody, setIsDraggingTextBody] = useState(false);
  const [isDraggingSubtitleStart, setIsDraggingSubtitleStart] = useState(false);
  const [isDraggingSubtitleEnd, setIsDraggingSubtitleEnd] = useState(false);
  const [isDraggingSubtitleBody, setIsDraggingSubtitleBody] = useState(false);
  const [draggingTextId, setDraggingTextId] = useState<string | null>(null);
  const [draggingSubtitleId, setDraggingSubtitleId] = useState<string | null>(null);
  const textDragOffsetRef = useRef(0);
  const subtitleDragOffsetRef = useRef(0);
  const textDragIdsRef = useRef<string[]>([]);
  const subtitleDragIdsRef = useRef<string[]>([]);
  const textDragOriginalsRef = useRef<Map<string, DragOriginalBounds> | null>(null);
  const subtitleDragOriginalsRef = useRef<Map<string, DragOriginalBounds> | null>(null);
  const pendingSubtitleDragClientXRef = useRef<number | null>(null);
  const subtitleDragFrameRef = useRef<number | null>(null);

  // Keystroke / pointer / webcam share an identical visibility-segment drag
  // machine; each instance differs only in which field it reads/writes and a
  // few side effects.
  const keystrokeDrag = useVisibilitySegmentDrag({
    duration,
    segment,
    setSegment,
    getTimeFromClientX,
    beginBatch,
    commitBatch,
    dragStartGuard: (seg) => !!seg && (seg.keystrokeMode ?? 'off') !== 'off',
    snapshotOriginals: (seg) =>
      clampVisibilitySegmentsToDuration(
        getKeystrokeVisibilitySegmentsForMode(seg as NonNullable<typeof seg>),
        duration,
      ).map((s) => ({ ...s })),
    onDragStart: (id) => {
      setEditingKeystrokeId?.(id);
    },
    writeDuringDrag: (seg, modified) =>
      withKeystrokeVisibilitySegmentsForMode(
        seg,
        clampVisibilitySegmentsToDuration(mergePointerSegments(modified), duration),
      ),
    getSegmentsForSplit: (seg) =>
      !seg || (seg.keystrokeMode ?? 'off') === 'off'
        ? null
        : clampVisibilitySegmentsToDuration(getKeystrokeVisibilitySegmentsForMode(seg), duration),
    setSegmentsAfterSplit: (seg, nextSegs) =>
      withKeystrokeVisibilitySegmentsForMode(
        seg,
        clampVisibilitySegmentsToDuration(nextSegs, duration),
      ),
    onSplit: () => {
      setEditingKeystrokeId?.(null);
    },
  });

  const pointerDrag = useVisibilitySegmentDrag({
    duration,
    segment,
    setSegment,
    getTimeFromClientX,
    beginBatch,
    commitBatch,
    snapshotOriginals: (seg) =>
      seg?.cursorVisibilitySegments
        ? clampVisibilitySegmentsToDuration(seg.cursorVisibilitySegments, duration).map((s) => ({ ...s }))
        : null,
    onDragStart: () => {
      setEditingKeystrokeId?.(null);
    },
    writeDuringDrag: (seg, modified) => ({
      ...seg,
      cursorVisibilitySegments: clampVisibilitySegmentsToDuration(mergePointerSegments(modified), duration),
    }),
    getSegmentsForSplit: (seg) =>
      seg?.cursorVisibilitySegments
        ? clampVisibilitySegmentsToDuration(seg.cursorVisibilitySegments, duration)
        : null,
    setSegmentsAfterSplit: (seg, nextSegs) => ({
      ...seg,
      cursorVisibilitySegments: nextSegs,
    }),
    onSplit: () => {
      setEditingPointerId?.(null);
    },
  });

  const webcamDrag = useVisibilitySegmentDrag({
    duration,
    segment,
    setSegment,
    getTimeFromClientX,
    beginBatch,
    commitBatch,
    snapshotOriginals: (seg) =>
      seg?.webcamVisibilitySegments
        ? clampVisibilitySegmentsToDuration(seg.webcamVisibilitySegments, duration).map((s) => ({ ...s }))
        : null,
    onDragStart: () => {
      setEditingKeystrokeId?.(null);
    },
    writeDuringDrag: (seg, modified) => ({
      ...seg,
      webcamVisibilitySegments: clampVisibilitySegmentsToDuration(mergePointerSegments(modified), duration),
    }),
    getSegmentsForSplit: (seg) =>
      seg?.webcamVisibilitySegments
        ? clampVisibilitySegmentsToDuration(seg.webcamVisibilitySegments, duration)
        : null,
    setSegmentsAfterSplit: (seg, nextSegs) => ({
      ...seg,
      webcamVisibilitySegments: nextSegs,
    }),
    onSplit: () => {},
  });

  const resolveActiveDragIds = useCallback((
    id: string,
    selectedIds: readonly string[],
    availableIds: readonly string[],
  ): string[] => {
    if (!selectedIds.includes(id)) return [id];
    const availableSet = new Set(availableIds);
    const resolved = selectedIds.filter((selectedId) => availableSet.has(selectedId));
    return resolved.length > 0 ? resolved : [id];
  }, []);

  // Text drag
  const handleTextDragStart = useCallback((id: string, type: 'start' | 'end' | 'body', offset?: number) => {
    beginBatch();
    setEditingTextId(id);
    setEditingKeystrokeId?.(null);
    setActivePanel('text');
    setDraggingTextId(id);
    textDragIdsRef.current = [id];
    textDragOriginalsRef.current = null;
    if (type === 'start') setIsDraggingTextStart(true);
    else if (type === 'end') setIsDraggingTextEnd(true);
    else if (type === 'body') {
      setIsDraggingTextBody(true);
      if (offset !== undefined) textDragOffsetRef.current = offset;
      if (segment) {
        const activeIds = resolveActiveDragIds(
          id,
          selectedTextIds,
          segment.textSegments.map((text) => text.id),
        );
        textDragIdsRef.current = activeIds;
        textDragOriginalsRef.current = snapshotSegmentBounds(segment.textSegments, activeIds);
      }
    }
  }, [beginBatch, resolveActiveDragIds, segment, selectedTextIds, setEditingTextId, setEditingKeystrokeId, setActivePanel]);

  const handleTextDrag = useCallback((clientX: number) => {
    if ((!isDraggingTextStart && !isDraggingTextEnd && !isDraggingTextBody) || !draggingTextId || !segment) return;
    const newTime = getTimeFromClientX(clientX);
    if (newTime === null) return;
    const activeTextIds = new Set(textDragIdsRef.current);

    setSegment({
      ...segment,
      textSegments: segment.textSegments.map(text => {
        if (isDraggingTextBody) {
          if (!activeTextIds.has(text.id)) return text;
        } else if (text.id !== draggingTextId) {
          return text;
        }
        if (isDraggingTextStart) {
          return { ...text, startTime: clampSegmentEdge(text, 'start', newTime, duration) };
        } else if (isDraggingTextEnd) {
          return { ...text, endTime: clampSegmentEdge(text, 'end', newTime, duration) };
        } else if (isDraggingTextBody) {
          const originals = textDragOriginalsRef.current;
          const activeIds = textDragIdsRef.current;
          if (!originals || activeIds.length === 0) {
            const dur = text.endTime - text.startTime;
            let newStart = newTime - textDragOffsetRef.current;
            if (newStart < 0) newStart = 0;
            if (newStart + dur > duration) newStart = duration - dur;
            return { ...text, startTime: newStart, endTime: newStart + dur };
          }
          const original = originals.get(text.id);
          const delta = computeGroupDragDelta(
            originals,
            draggingTextId,
            textDragOffsetRef.current,
            newTime,
            duration,
          );
          if (!original || delta === null) return text;
          return {
            ...text,
            startTime: original.startTime + delta,
            endTime: original.endTime + delta,
          };
        }
        return text;
      }),
    });
  }, [isDraggingTextStart, isDraggingTextEnd, isDraggingTextBody, draggingTextId, segment, getTimeFromClientX, setSegment, duration]);

  const handleSubtitleDragStart = useCallback((id: string, type: 'start' | 'end' | 'body', offset?: number) => {
    beginBatch();
    setEditingSubtitleId?.(id);
    setEditingTextId(null);
    setEditingKeystrokeId?.(null);
    setActivePanel('subtitles');
    setDraggingSubtitleId(id);
    subtitleDragIdsRef.current = [id];
    subtitleDragOriginalsRef.current = null;
    if (type === 'start') setIsDraggingSubtitleStart(true);
    else if (type === 'end') setIsDraggingSubtitleEnd(true);
    else if (type === 'body') {
      setIsDraggingSubtitleBody(true);
      if (offset !== undefined) subtitleDragOffsetRef.current = offset;
      if (segment) {
        const activeIds = resolveActiveDragIds(
          id,
          selectedSubtitleIds,
          getVisibleSubtitleSegments(segment).map((subtitle) => subtitle.id),
        );
        subtitleDragIdsRef.current = activeIds;
        subtitleDragOriginalsRef.current = snapshotSegmentBounds(
          getVisibleSubtitleSegments(segment),
          activeIds,
        );
      }
    }
  }, [beginBatch, resolveActiveDragIds, segment, selectedSubtitleIds, setEditingSubtitleId, setEditingTextId, setEditingKeystrokeId, setActivePanel]);

  const applySubtitleDrag = useCallback((clientX: number) => {
    if ((!isDraggingSubtitleStart && !isDraggingSubtitleEnd && !isDraggingSubtitleBody) || !draggingSubtitleId || !segment) return;
    const newTime = getTimeFromClientX(clientX);
    if (newTime === null) return;

    const updater = (subtitle: SubtitleSegment) => {
      if (isDraggingSubtitleStart) {
        return { ...subtitle, startTime: clampSegmentEdge(subtitle, 'start', newTime, duration) };
      }
      if (isDraggingSubtitleEnd) {
        return { ...subtitle, endTime: clampSegmentEdge(subtitle, 'end', newTime, duration) };
      }
      if (isDraggingSubtitleBody) {
        const originals = subtitleDragOriginalsRef.current;
        const activeIds = subtitleDragIdsRef.current;
        if (!originals || activeIds.length === 0) {
          const dur = subtitle.endTime - subtitle.startTime;
          let newStart = newTime - subtitleDragOffsetRef.current;
          if (newStart < 0) newStart = 0;
          if (newStart + dur > duration) newStart = duration - dur;
          return { ...subtitle, startTime: newStart, endTime: newStart + dur };
        }
        const original = originals.get(subtitle.id);
        const delta = computeGroupDragDelta(
          originals,
          draggingSubtitleId,
          subtitleDragOffsetRef.current,
          newTime,
          duration,
        );
        if (!original || delta === null) return subtitle;
        return {
          ...subtitle,
          startTime: original.startTime + delta,
          endTime: original.endTime + delta,
        };
      }
      return subtitle;
    };

    setSegment(
      isDraggingSubtitleBody
        ? updateSubtitleTimingsAcrossTracks(segment, subtitleDragIdsRef.current, updater)
        : updateSubtitleTimingAcrossTracks(segment, draggingSubtitleId, updater),
    );
  }, [isDraggingSubtitleStart, isDraggingSubtitleEnd, isDraggingSubtitleBody, draggingSubtitleId, segment, getTimeFromClientX, setSegment, duration]);

  const handleSubtitleDrag = useCallback((clientX: number) => {
    pendingSubtitleDragClientXRef.current = clientX;
    if (subtitleDragFrameRef.current !== null) return;
    subtitleDragFrameRef.current = requestAnimationFrame(() => {
      subtitleDragFrameRef.current = null;
      const pendingClientX = pendingSubtitleDragClientXRef.current;
      pendingSubtitleDragClientXRef.current = null;
      if (pendingClientX !== null) {
        applySubtitleDrag(pendingClientX);
      }
    });
  }, [applySubtitleDrag]);

  // Text click (select)
  const handleTextClick = useCallback((id: string) => {
    if (!isDraggingTextStart && !isDraggingTextEnd) {
      setEditingTextId(id);
      setEditingKeystrokeId?.(null);
      setActivePanel('text');
    }
  }, [isDraggingTextStart, isDraggingTextEnd, setEditingTextId, setEditingKeystrokeId, setActivePanel]);

  const handleSubtitleClick = useCallback((id: string) => {
    if (!isDraggingSubtitleStart && !isDraggingSubtitleEnd) {
      setEditingSubtitleId?.(id);
      setEditingTextId(null);
      setEditingKeystrokeId?.(null);
      setActivePanel('subtitles');
    }
  }, [isDraggingSubtitleStart, isDraggingSubtitleEnd, setEditingSubtitleId, setEditingTextId, setEditingKeystrokeId, setActivePanel]);

  const resetTrackDragState = useCallback(() => {
    const pendingClientX = pendingSubtitleDragClientXRef.current;
    if (subtitleDragFrameRef.current !== null) {
      cancelAnimationFrame(subtitleDragFrameRef.current);
      subtitleDragFrameRef.current = null;
    }
    if (pendingClientX !== null) {
      applySubtitleDrag(pendingClientX);
    }
    pendingSubtitleDragClientXRef.current = null;
    setIsDraggingTextStart(false);
    setIsDraggingTextEnd(false);
    setIsDraggingTextBody(false);
    setIsDraggingSubtitleStart(false);
    setIsDraggingSubtitleEnd(false);
    setIsDraggingSubtitleBody(false);
    keystrokeDrag.reset();
    pointerDrag.reset();
    webcamDrag.reset();
    setDraggingTextId(null);
    setDraggingSubtitleId(null);
    textDragIdsRef.current = [];
    subtitleDragIdsRef.current = [];
    textDragOriginalsRef.current = null;
    subtitleDragOriginalsRef.current = null;
  }, [applySubtitleDrag, keystrokeDrag.reset, pointerDrag.reset, webcamDrag.reset]);

  return {
    // State
    isDraggingTextStart,
    isDraggingTextEnd,
    isDraggingTextBody,
    isDraggingSubtitleStart,
    isDraggingSubtitleEnd,
    isDraggingSubtitleBody,
    isDraggingKeystrokeStart: keystrokeDrag.isDraggingStart,
    isDraggingKeystrokeEnd: keystrokeDrag.isDraggingEnd,
    isDraggingKeystrokeBody: keystrokeDrag.isDraggingBody,
    isDraggingPointerStart: pointerDrag.isDraggingStart,
    isDraggingPointerEnd: pointerDrag.isDraggingEnd,
    isDraggingPointerBody: pointerDrag.isDraggingBody,
    isDraggingWebcamStart: webcamDrag.isDraggingStart,
    isDraggingWebcamEnd: webcamDrag.isDraggingEnd,
    isDraggingWebcamBody: webcamDrag.isDraggingBody,
    draggingTextId,
    draggingSubtitleId,
    draggingKeystrokeId: keystrokeDrag.draggingId,
    draggingPointerId: pointerDrag.draggingId,
    draggingWebcamId: webcamDrag.draggingId,
    // Handlers
    handleTextDragStart,
    handleTextDrag,
    handleTextClick,
    handleSubtitleDragStart,
    handleSubtitleDrag,
    handleSubtitleClick,
    handleKeystrokeDragStart: keystrokeDrag.handleDragStart,
    handleKeystrokeDrag: keystrokeDrag.handleDrag,
    handleKeystrokeClick: keystrokeDrag.handleClick,
    handlePointerDragStart: pointerDrag.handleDragStart,
    handlePointerDrag: pointerDrag.handleDrag,
    handlePointerClick: pointerDrag.handleClick,
    handleWebcamDragStart: webcamDrag.handleDragStart,
    handleWebcamDrag: webcamDrag.handleDrag,
    handleWebcamClick: webcamDrag.handleClick,
    resetTrackDragState,
  };
}
