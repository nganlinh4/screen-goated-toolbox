import { useState, useCallback, useEffect, useRef } from 'react';
import {
  normalizeSelectionRange,
  type TrackSelectionRange,
} from '@/lib/timelineSegmentSelection';

export interface RangeSelectState {
  startX: number;
  endX: number;
  startTime: number;
  endTime: number;
}

export interface TrackRangeSelect {
  selectedIds: Set<string>;
  selectedRange: TrackSelectionRange | null;
  rangeSelect: RangeSelectState | null;
  activeDragMode: 'sweep-select' | 'ctrl-range' | null;
  trackRef: React.RefObject<HTMLDivElement>;
  isDraggingRange: React.MutableRefObject<boolean>;
  clearSelection: () => void;
  onSegmentPointerDown: () => void;
  addSegmentSelection: (
    id: string,
    options?: { shiftKey?: boolean; ctrlKey?: boolean },
  ) => void;
  handleTrackPointerDown: (e: React.PointerEvent<HTMLDivElement>) => void;
  handleTrackPointerMove: (e: React.PointerEvent<HTMLDivElement>) => void;
  handleTrackPointerUp: (e: React.PointerEvent<HTMLDivElement>) => void;
}

/**
 * Shared drag-to-select hook for segment-based timeline tracks.
 * Supports additive click selection plus explicit swept-range selection.
 */
export function useTrackRangeSelect<T extends { id: string; startTime: number; endTime: number }>(
  segments: T[],
  duration: number,
  onSelectionChange?: (ids: string[]) => void,
  onRangeChange?: (range: TrackSelectionRange | null) => void,
  onDeleteSelected?: (ids: string[]) => void,
  clearSignal?: number,
  options?: {
    allowBackgroundRangeSelect?: boolean;
    allowCtrlDragAnywhere?: boolean;
  },
): TrackRangeSelect {
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [selectedRange, setSelectedRange] = useState<TrackSelectionRange | null>(null);
  const [rangeSelect, setRangeSelect] = useState<RangeSelectState | null>(null);
  const trackRef = useRef<HTMLDivElement>(null);
  const isDraggingRange = useRef(false);
  const preDragSnapshot = useRef<Set<string>>(new Set());
  const dragMode = useRef<'sweep-select' | 'ctrl-range' | null>(null);

  // External clear signal — parent increments to force clear
  const lastClearSignal = useRef(clearSignal ?? 0);
  useEffect(() => {
    if (clearSignal !== undefined && clearSignal !== lastClearSignal.current) {
      lastClearSignal.current = clearSignal;
      setSelectedIds(new Set());
      setSelectedRange(null);
    }
  }, [clearSignal]);

  const safeDuration = Math.max(duration, 0.001);
  const allowBackgroundRangeSelect = options?.allowBackgroundRangeSelect ?? true;
  const allowCtrlDragAnywhere = options?.allowCtrlDragAnywhere ?? false;

  const getSweptIds = useCallback((t1: number, t2: number): Set<string> => {
    const lo = Math.min(t1, t2);
    const hi = Math.max(t1, t2);
    const ids = new Set<string>();
    for (const seg of segments) {
      if (seg.endTime > lo && seg.startTime < hi) ids.add(seg.id);
    }
    return ids;
  }, [segments]);

  const clearSelection = useCallback(() => {
    setSelectedIds(new Set());
    setSelectedRange(null);
  }, []);

  const addSegmentSelection = useCallback((
    id: string,
    options?: { shiftKey?: boolean; ctrlKey?: boolean },
  ) => {
    const target = segments.find((segment) => segment.id === id);
    if (!target) return;
    setSelectedIds((prev) => {
      if (options?.shiftKey) {
        const next = new Set(prev);
        if (next.has(id)) next.delete(id);
        else next.add(id);
        return next;
      }
      return new Set([id]);
    });
  }, [segments]);

  const onSegmentPointerDown = useCallback(() => {}, []);

  // Notify parent when selection changes
  useEffect(() => {
    onSelectionChange?.(Array.from(selectedIds));
  }, [selectedIds, onSelectionChange]);

  useEffect(() => {
    onRangeChange?.(selectedRange);
  }, [selectedRange, onRangeChange]);

  // Delete key handler
  useEffect(() => {
    if (selectedIds.size === 0) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement;
      if (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable) return;
      if ((e.code === 'Delete' || e.code === 'Backspace') && selectedIds.size > 0) {
        e.preventDefault();
        e.stopPropagation();
        onDeleteSelected?.(Array.from(selectedIds));
        setSelectedIds(new Set());
      }
      if (e.code === 'Escape') {
        setSelectedIds(new Set());
        setSelectedRange(null);
      }
    };
    window.addEventListener('keydown', handleKeyDown, true);
    return () => window.removeEventListener('keydown', handleKeyDown, true);
  }, [selectedIds, onDeleteSelected]);

  const handleTrackPointerDown = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    const isCtrlRangeGesture = allowCtrlDragAnywhere && e.ctrlKey;
    if (!isCtrlRangeGesture && e.target !== e.currentTarget) return;
    if (!isCtrlRangeGesture && !allowBackgroundRangeSelect) return;
    const rect = e.currentTarget.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const time = (x / rect.width) * safeDuration;
    if (!isCtrlRangeGesture) {
      const onSeg = segments.some(seg => time >= seg.startTime && time <= seg.endTime);
      if (onSeg) return;
    }

    isDraggingRange.current = true;
    dragMode.current = isCtrlRangeGesture ? 'ctrl-range' : 'sweep-select';
    preDragSnapshot.current = new Set(selectedIds);
    setRangeSelect({ startX: x, endX: x, startTime: time, endTime: time });
    e.preventDefault();
    e.stopPropagation();
    e.currentTarget.setPointerCapture(e.pointerId);
  }, [allowBackgroundRangeSelect, allowCtrlDragAnywhere, safeDuration, segments, selectedIds]);

  const handleTrackPointerMove = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    if (!isDraggingRange.current || !rangeSelect) return;
    const rect = e.currentTarget.getBoundingClientRect();
    const x = Math.max(0, Math.min(rect.width, e.clientX - rect.left));
    const time = (x / rect.width) * safeDuration;
    setRangeSelect(prev => prev ? { ...prev, endX: x, endTime: time } : null);
    if (dragMode.current === 'sweep-select') {
      const swept = getSweptIds(rangeSelect.startTime, time);
      setSelectedIds(swept);
      setSelectedRange(null);
    }
    e.stopPropagation();
  }, [rangeSelect, safeDuration, getSweptIds]);

  const handleTrackPointerUp = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    if (!isDraggingRange.current) return;
    isDraggingRange.current = false;
    e.stopPropagation();
    e.currentTarget.releasePointerCapture(e.pointerId);

    // If barely dragged, treat as no-op (don't change selection)
    if (rangeSelect && Math.abs(rangeSelect.endX - rangeSelect.startX) < 4) {
      setSelectedIds(preDragSnapshot.current);
      if (dragMode.current === 'ctrl-range') {
        setSelectedRange(null);
      }
    } else if (rangeSelect) {
      const normalizedRange = normalizeSelectionRange(
        {
          startTime: rangeSelect.startTime,
          endTime: rangeSelect.endTime,
        },
        'drag',
      );
      if (normalizedRange) {
        if (dragMode.current === 'ctrl-range') {
          setSelectedRange(normalizedRange);
        } else {
          setSelectedIds(
            getSweptIds(normalizedRange.startTime, normalizedRange.endTime),
          );
        }
      }
    }
    dragMode.current = null;
    setRangeSelect(null);
  }, [getSweptIds, rangeSelect]);

  return {
    selectedIds,
    selectedRange,
    rangeSelect,
    activeDragMode: dragMode.current,
    trackRef,
    isDraggingRange,
    clearSelection,
    onSegmentPointerDown,
    addSegmentSelection,
    handleTrackPointerDown,
    handleTrackPointerMove,
    handleTrackPointerUp,
  };
}
