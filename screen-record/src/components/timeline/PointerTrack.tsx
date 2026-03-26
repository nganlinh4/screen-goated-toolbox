import React, { useState, useCallback, useEffect, useRef } from 'react';
import { Scissors } from 'lucide-react';
import { VideoSegment, CursorVisibilitySegment } from '@/types/video';
import { clampVisibilitySegmentsToDuration } from '@/lib/cursorHiding';
import {
  getHandlePriorityThresholdTime,
  isTimeNearRangeBoundary,
} from "./trackHoverUtils";

interface PointerTrackProps {
  segment: VideoSegment;
  duration: number;
  onPointerClick: (id: string, splitTime: number) => void;
  onHandleDragStart: (id: string, type: 'start' | 'end' | 'body', offset?: number) => void;
  onAddPointerSegment?: (atTime?: number) => void;
  onPointerHover?: (id: string | null) => void;
  onDeletePointerSegments?: (ids: string[]) => void;
}

export const PointerTrack: React.FC<PointerTrackProps> = ({
  segment,
  duration,
  onPointerClick,
  onHandleDragStart,
  onAddPointerSegment,
  onPointerHover,
  onDeletePointerSegments,
}) => {
  const [hoverState, setHoverState] = useState<
    | { type: 'split'; x: number; time: number; seg: CursorVisibilitySegment }
    | { type: 'add'; x: number }
    | null
  >(null);

  // Range-select drag state
  const [rangeSelect, setRangeSelect] = useState<{
    startX: number; endX: number; startTime: number; endTime: number;
  } | null>(null);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const trackRef = useRef<HTMLDivElement>(null);
  const isDraggingRange = useRef(false);

  const safeDuration = Math.max(duration, 0.001);
  const segments = clampVisibilitySegmentsToDuration(segment.cursorVisibilitySegments, safeDuration);

  // Compute which segments fall in the selection range
  const getSelectedInRange = useCallback((t1: number, t2: number) => {
    const lo = Math.min(t1, t2);
    const hi = Math.max(t1, t2);
    const ids = new Set<string>();
    for (const seg of segments) {
      if (seg.endTime > lo && seg.startTime < hi) ids.add(seg.id);
    }
    return ids;
  }, [segments]);

  const handleMouseMove = (e: React.MouseEvent<HTMLDivElement>) => {
    if (isDraggingRange.current) return;
    const rect = e.currentTarget.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const time = (x / rect.width) * safeDuration;
    const thresholdTime = getHandlePriorityThresholdTime(safeDuration, rect.width);

    const containing = segments.find(
      seg => time >= seg.startTime && time <= seg.endTime
    );
    if (containing) {
      const canSplit = time > containing.startTime + 0.15 && time < containing.endTime - 0.15;
      setHoverState(canSplit ? { type: 'split', x, time, seg: containing } : null);
      return;
    }
    if (isTimeNearRangeBoundary(time, segments, thresholdTime)) {
      setHoverState(null);
      return;
    }
    setHoverState({ type: 'add', x });
  };

  // Start range-select drag on empty area
  const handleTrackPointerDown = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    // Only on the track background, not on segments or buttons
    if (e.target !== e.currentTarget) return;
    const rect = e.currentTarget.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const time = (x / rect.width) * safeDuration;
    // Check if on a segment — if so, don't range-select
    const onSeg = segments.some(seg => time >= seg.startTime && time <= seg.endTime);
    if (onSeg) return;

    isDraggingRange.current = true;
    setHoverState(null);
    setRangeSelect({ startX: x, endX: x, startTime: time, endTime: time });
    setSelectedIds(new Set());
    e.currentTarget.setPointerCapture(e.pointerId);
  }, [safeDuration, segments]);

  const handleTrackPointerMove = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    if (!isDraggingRange.current || !rangeSelect) return;
    const rect = e.currentTarget.getBoundingClientRect();
    const x = Math.max(0, Math.min(rect.width, e.clientX - rect.left));
    const time = (x / rect.width) * safeDuration;
    setRangeSelect(prev => prev ? { ...prev, endX: x, endTime: time } : null);
    setSelectedIds(getSelectedInRange(rangeSelect.startTime, time));
  }, [rangeSelect, safeDuration, getSelectedInRange]);

  const handleTrackPointerUp = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    if (!isDraggingRange.current) return;
    isDraggingRange.current = false;
    e.currentTarget.releasePointerCapture(e.pointerId);

    // If barely dragged (< 4px), treat as click for add
    if (rangeSelect && Math.abs(rangeSelect.endX - rangeSelect.startX) < 4) {
      setRangeSelect(null);
      setSelectedIds(new Set());
      return;
    }
    // Keep selection visible, clear the drag line
    setRangeSelect(null);
  }, [rangeSelect]);

  // Delete key handler for selected segments
  useEffect(() => {
    if (selectedIds.size === 0) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.code === 'Delete' || e.code === 'Backspace') && selectedIds.size > 0) {
        const target = e.target as HTMLElement;
        if (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable) return;
        e.preventDefault();
        e.stopPropagation();
        onDeletePointerSegments?.(Array.from(selectedIds));
        setSelectedIds(new Set());
      }
      if (e.code === 'Escape') {
        setSelectedIds(new Set());
      }
    };
    window.addEventListener('keydown', handleKeyDown, true);
    return () => window.removeEventListener('keydown', handleKeyDown, true);
  }, [selectedIds, onDeletePointerSegments]);

  // Clear selection when clicking another track's segment (not SidePanel controls).
  // Escape key handles intentional deselection.

  const rangeLeft = rangeSelect ? Math.min(rangeSelect.startX, rangeSelect.endX) : 0;
  const rangeWidth = rangeSelect ? Math.abs(rangeSelect.endX - rangeSelect.startX) : 0;

  return (
    <div
      ref={trackRef}
      className="pointer-track timeline-lane relative h-7"
      onMouseMove={handleMouseMove}
      onMouseLeave={() => { if (!isDraggingRange.current) setHoverState(null); }}
      onPointerDown={handleTrackPointerDown}
      onPointerMove={handleTrackPointerMove}
      onPointerUp={handleTrackPointerUp}
    >
      {segments.map((seg) => (
        <div
          key={seg.id}
          onPointerDown={(e) => {
            e.stopPropagation();
            setSelectedIds(new Set());
            const rect = e.currentTarget.parentElement!.getBoundingClientRect();
            const clickX = e.clientX - rect.left;
            const clickTime = (clickX / rect.width) * safeDuration;
            onHandleDragStart(seg.id, 'body', clickTime - seg.startTime);
          }}
          onMouseEnter={() => onPointerHover?.(seg.id)}
          onMouseLeave={() => onPointerHover?.(null)}
          className="pointer-segment timeline-block absolute h-full cursor-move group"
          data-tone="warning"
          data-selected={selectedIds.has(seg.id) ? "true" : undefined}
          style={{
            left: `${(seg.startTime / safeDuration) * 100}%`,
            width: `${((seg.endTime - seg.startTime) / safeDuration) * 100}%`,
          }}
        >
          <div className="pointer-segment-content absolute inset-0 flex items-center justify-center overflow-hidden px-1">
            <span className="pointer-segment-icon text-[10px] text-[var(--timeline-warning-color)] truncate">
              ●
            </span>
          </div>
          <div
            className="pointer-handle-start absolute inset-y-0 -left-[2px] w-[5px] cursor-ew-resize flex items-center justify-center opacity-0 group-hover:opacity-100 z-10"
            onPointerDown={(e) => { e.stopPropagation(); onHandleDragStart(seg.id, 'start'); }}
          >
            <div className="pointer-handle-bar timeline-handle-pill" />
          </div>
          <div
            className="pointer-handle-end absolute inset-y-0 -right-[2px] w-[5px] cursor-ew-resize flex items-center justify-center opacity-0 group-hover:opacity-100 z-10"
            onPointerDown={(e) => { e.stopPropagation(); onHandleDragStart(seg.id, 'end'); }}
          >
            <div className="pointer-handle-bar timeline-handle-pill" />
          </div>
        </div>
      ))}

      {/* Range-select drag line */}
      {rangeSelect && rangeWidth > 2 && (
        <div
          className="pointer-range-select timeline-range-select absolute pointer-events-none z-5"
          style={{ left: rangeLeft, width: rangeWidth }}
        />
      )}

      {/* Selected count badge */}
      {selectedIds.size > 0 && !rangeSelect && (
        <div
          className="pointer-selection-badge absolute top-0 right-1 text-[8px] font-bold px-1 rounded-b-sm pointer-events-none z-20"
          style={{ background: 'var(--timeline-zoom-color)', color: 'var(--timeline-float-fg)' }}
        >
          {selectedIds.size} selected · Del
        </div>
      )}

      {hoverState && hoverState.type === 'split' && !isDraggingRange.current && (
        <button
          className="pointer-split-btn timeline-arch-button absolute bottom-0 z-10 pointer-events-auto flex items-center justify-center"
          data-tone="accent"
          style={{ left: hoverState.x - 8 }}
          onPointerDown={(e) => {
            e.stopPropagation();
            onPointerClick(hoverState.seg.id, hoverState.time);
            setHoverState(null);
          }}
        >
          <Scissors className="w-2 h-2" />
        </button>
      )}
      {hoverState && hoverState.type === 'add' && onAddPointerSegment && !isDraggingRange.current && (
        <button
          className="pointer-add-btn timeline-arch-button absolute bottom-0 z-10 pointer-events-auto flex items-center justify-center text-[8px] font-bold"
          data-tone="warning"
          style={{ left: hoverState.x - 8 }}
          onPointerDown={(e) => {
            e.stopPropagation();
            const rect = e.currentTarget.parentElement!.getBoundingClientRect();
            const time = (hoverState.x / rect.width) * safeDuration;
            onAddPointerSegment(time);
            setHoverState(null);
          }}
        >
          +
        </button>
      )}
    </div>
  );
};
