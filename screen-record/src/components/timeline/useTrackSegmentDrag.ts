import { useState, useCallback, useRef } from 'react';
import type { CursorVisibilitySegment, SubtitleSegment } from '@/types/video';
import { clampVisibilitySegmentsToDuration, mergePointerSegments } from '@/lib/cursorHiding';
import { getKeystrokeVisibilitySegmentsForMode, withKeystrokeVisibilitySegmentsForMode } from '@/lib/keystrokeVisibility';
import { updateSubtitleTimingAcrossTracks, updateSubtitleTimingsAcrossTracks } from '@/lib/subtitleTrackMutations';
import { getVisibleSubtitleSegments } from '@/lib/subtitleTracks';
import { computeGroupDragDelta, snapshotSegmentBounds, type DragOriginalBounds } from './trackDragUtils';
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
  const [draggingSubtitleId, setDraggingSubtitleId] = useState<string | null>(null);
  const [draggingKeystrokeId, setDraggingKeystrokeId] = useState<string | null>(null);
  const [draggingPointerId, setDraggingPointerId] = useState<string | null>(null);
  const [draggingWebcamId, setDraggingWebcamId] = useState<string | null>(null);
  const textDragOffsetRef = useRef(0);
  const subtitleDragOffsetRef = useRef(0);
  const textDragIdsRef = useRef<string[]>([]);
  const subtitleDragIdsRef = useRef<string[]>([]);
  const textDragOriginalsRef = useRef<Map<string, DragOriginalBounds> | null>(null);
  const subtitleDragOriginalsRef = useRef<Map<string, DragOriginalBounds> | null>(null);
  const pendingSubtitleDragClientXRef = useRef<number | null>(null);
  const subtitleDragFrameRef = useRef<number | null>(null);
  const keystrokeDragOffsetRef = useRef(0);
  const pointerDragOffsetRef = useRef(0);
  const webcamDragOffsetRef = useRef(0);
  const keystrokeDragOriginals = useRef<CursorVisibilitySegment[] | null>(null);
  const pointerDragOriginals = useRef<CursorVisibilitySegment[] | null>(null);
  const webcamDragOriginals = useRef<CursorVisibilitySegment[] | null>(null);
  const keystrokeDragDidMove = useRef(false);
  const pointerDragDidMove = useRef(false);
  const webcamDragDidMove = useRef(false);

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
          return { ...text, startTime: Math.min(Math.max(0, newTime), text.endTime - 0.1) };
        } else if (isDraggingTextEnd) {
          return { ...text, endTime: Math.max(Math.min(duration, newTime), text.startTime + 0.1) };
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
        return { ...subtitle, startTime: Math.min(Math.max(0, newTime), subtitle.endTime - 0.1) };
      }
      if (isDraggingSubtitleEnd) {
        return { ...subtitle, endTime: Math.max(Math.min(duration, newTime), subtitle.startTime + 0.1) };
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

  const handleSubtitleClick = useCallback((id: string) => {
    if (!isDraggingSubtitleStart && !isDraggingSubtitleEnd) {
      setEditingSubtitleId?.(id);
      setEditingTextId(null);
      setEditingKeystrokeId?.(null);
      setActivePanel('subtitles');
    }
  }, [isDraggingSubtitleStart, isDraggingSubtitleEnd, setEditingSubtitleId, setEditingTextId, setEditingKeystrokeId, setActivePanel]);

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
    setDraggingSubtitleId(null);
    textDragIdsRef.current = [];
    subtitleDragIdsRef.current = [];
    textDragOriginalsRef.current = null;
    subtitleDragOriginalsRef.current = null;
    setDraggingKeystrokeId(null);
    setDraggingPointerId(null);
    setDraggingWebcamId(null);
    keystrokeDragOriginals.current = null;
    pointerDragOriginals.current = null;
    webcamDragOriginals.current = null;
  }, [applySubtitleDrag]);

  return {
    // State
    isDraggingTextStart,
    isDraggingTextEnd,
    isDraggingTextBody,
    isDraggingSubtitleStart,
    isDraggingSubtitleEnd,
    isDraggingSubtitleBody,
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
    draggingSubtitleId,
    draggingKeystrokeId,
    draggingPointerId,
    draggingWebcamId,
    // Handlers
    handleTextDragStart,
    handleTextDrag,
    handleTextClick,
    handleSubtitleDragStart,
    handleSubtitleDrag,
    handleSubtitleClick,
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
