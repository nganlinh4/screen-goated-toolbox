import { useState, useCallback, useEffect, useRef } from 'react';

export interface RangeSelectState {
  startX: number;
  endX: number;
  startTime: number;
  endTime: number;
}

export interface TrackRangeSelect {
  selectedIds: Set<string>;
  rangeSelect: RangeSelectState | null;
  trackRef: React.RefObject<HTMLDivElement>;
  isDraggingRange: React.MutableRefObject<boolean>;
  clearSelection: () => void;
  onSegmentPointerDown: () => void;
  handleTrackPointerDown: (e: React.PointerEvent<HTMLDivElement>) => void;
  handleTrackPointerMove: (e: React.PointerEvent<HTMLDivElement>) => void;
  handleTrackPointerUp: (e: React.PointerEvent<HTMLDivElement>) => void;
}

/**
 * Shared drag-to-select hook for segment-based timeline tracks.
 * Supports toggle selection: each drag XORs swept segments with existing selection.
 */
export function useTrackRangeSelect<T extends { id: string; startTime: number; endTime: number }>(
  segments: T[],
  duration: number,
  onSelectionChange?: (ids: string[]) => void,
  onDeleteSelected?: (ids: string[]) => void,
  clearSignal?: number,
): TrackRangeSelect {
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [rangeSelect, setRangeSelect] = useState<RangeSelectState | null>(null);
  const trackRef = useRef<HTMLDivElement>(null);
  const isDraggingRange = useRef(false);
  const preToggleSnapshot = useRef<Set<string>>(new Set());

  // External clear signal — parent increments to force clear
  const lastClearSignal = useRef(clearSignal ?? 0);
  useEffect(() => {
    if (clearSignal !== undefined && clearSignal !== lastClearSignal.current) {
      lastClearSignal.current = clearSignal;
      setSelectedIds(new Set());
    }
  }, [clearSignal]);

  const safeDuration = Math.max(duration, 0.001);

  const getSweptIds = useCallback((t1: number, t2: number): Set<string> => {
    const lo = Math.min(t1, t2);
    const hi = Math.max(t1, t2);
    const ids = new Set<string>();
    for (const seg of segments) {
      if (seg.endTime > lo && seg.startTime < hi) ids.add(seg.id);
    }
    return ids;
  }, [segments]);

  // Toggle: XOR swept IDs with the pre-drag snapshot
  const applyToggle = useCallback((swept: Set<string>): Set<string> => {
    const result = new Set(preToggleSnapshot.current);
    for (const id of swept) {
      if (result.has(id)) result.delete(id);
      else result.add(id);
    }
    return result;
  }, []);

  const clearSelection = useCallback(() => {
    setSelectedIds(new Set());
  }, []);

  // Called by track when a segment body is clicked (starts drag) — clears selection
  const onSegmentPointerDown = useCallback(() => {
    setSelectedIds(new Set());
  }, []);

  // Notify parent when selection changes
  useEffect(() => {
    onSelectionChange?.(Array.from(selectedIds));
  }, [selectedIds, onSelectionChange]);

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
      }
    };
    window.addEventListener('keydown', handleKeyDown, true);
    return () => window.removeEventListener('keydown', handleKeyDown, true);
  }, [selectedIds, onDeleteSelected]);

  const handleTrackPointerDown = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    if (e.target !== e.currentTarget) return;
    const rect = e.currentTarget.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const time = (x / rect.width) * safeDuration;
    // Don't start range-select if on a segment
    const onSeg = segments.some(seg => time >= seg.startTime && time <= seg.endTime);
    if (onSeg) return;

    isDraggingRange.current = true;
    preToggleSnapshot.current = new Set(selectedIds);
    setRangeSelect({ startX: x, endX: x, startTime: time, endTime: time });
    e.currentTarget.setPointerCapture(e.pointerId);
  }, [safeDuration, segments, selectedIds]);

  const handleTrackPointerMove = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    if (!isDraggingRange.current || !rangeSelect) return;
    const rect = e.currentTarget.getBoundingClientRect();
    const x = Math.max(0, Math.min(rect.width, e.clientX - rect.left));
    const time = (x / rect.width) * safeDuration;
    setRangeSelect(prev => prev ? { ...prev, endX: x, endTime: time } : null);
    // Live toggle preview during drag
    const swept = getSweptIds(rangeSelect.startTime, time);
    setSelectedIds(applyToggle(swept));
  }, [rangeSelect, safeDuration, getSweptIds, applyToggle]);

  const handleTrackPointerUp = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    if (!isDraggingRange.current) return;
    isDraggingRange.current = false;
    e.currentTarget.releasePointerCapture(e.pointerId);

    // If barely dragged, treat as no-op (don't change selection)
    if (rangeSelect && Math.abs(rangeSelect.endX - rangeSelect.startX) < 4) {
      setSelectedIds(preToggleSnapshot.current);
    }
    setRangeSelect(null);
  }, [rangeSelect]);

  return {
    selectedIds,
    rangeSelect,
    trackRef,
    isDraggingRange,
    clearSelection,
    onSegmentPointerDown,
    handleTrackPointerDown,
    handleTrackPointerMove,
    handleTrackPointerUp,
  };
}
