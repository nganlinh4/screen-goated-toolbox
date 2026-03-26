import React, { useState } from 'react';
import { Scissors } from 'lucide-react';
import { VideoSegment, CursorVisibilitySegment } from '@/types/video';
import { clampVisibilitySegmentsToDuration } from '@/lib/cursorHiding';
import {
  getHandlePriorityThresholdTime,
  isTimeNearRangeBoundary,
} from "./trackHoverUtils";
import { useTrackRangeSelect } from './useTrackRangeSelect';

interface PointerTrackProps {
  segment: VideoSegment;
  duration: number;
  onPointerClick: (id: string, splitTime: number) => void;
  onHandleDragStart: (id: string, type: 'start' | 'end' | 'body', offset?: number) => void;
  onAddPointerSegment?: (atTime?: number) => void;
  onPointerHover?: (id: string | null) => void;
  onDeletePointerSegments?: (ids: string[]) => void;
  onSelectionChange?: (ids: string[]) => void;
  clearSignal?: number;
}

export const PointerTrack: React.FC<PointerTrackProps> = ({
  segment,
  duration,
  onPointerClick,
  onHandleDragStart,
  onAddPointerSegment,
  onPointerHover,
  onDeletePointerSegments,
  onSelectionChange,
  clearSignal,
}) => {
  const [hoverState, setHoverState] = useState<
    | { type: 'split'; x: number; time: number; seg: CursorVisibilitySegment }
    | { type: 'add'; x: number }
    | null
  >(null);

  const safeDuration = Math.max(duration, 0.001);
  const segments = clampVisibilitySegmentsToDuration(segment.cursorVisibilitySegments, safeDuration);

  const {
    selectedIds, rangeSelect, trackRef, isDraggingRange,
    onSegmentPointerDown,
    handleTrackPointerDown, handleTrackPointerMove, handleTrackPointerUp,
  } = useTrackRangeSelect(segments, duration, onSelectionChange, onDeletePointerSegments, clearSignal);

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
            onSegmentPointerDown();
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
            <span className="pointer-segment-icon text-[10px] text-[var(--timeline-warning-color)] truncate">●</span>
          </div>
          <div className="pointer-handle-start absolute inset-y-0 -left-[2px] w-[5px] cursor-ew-resize flex items-center justify-center opacity-0 group-hover:opacity-100 z-10"
            onPointerDown={(e) => { e.stopPropagation(); onHandleDragStart(seg.id, 'start'); }}>
            <div className="pointer-handle-bar timeline-handle-pill" />
          </div>
          <div className="pointer-handle-end absolute inset-y-0 -right-[2px] w-[5px] cursor-ew-resize flex items-center justify-center opacity-0 group-hover:opacity-100 z-10"
            onPointerDown={(e) => { e.stopPropagation(); onHandleDragStart(seg.id, 'end'); }}>
            <div className="pointer-handle-bar timeline-handle-pill" />
          </div>
        </div>
      ))}

      {rangeSelect && rangeWidth > 2 && (
        <div className="pointer-range-select timeline-range-select absolute pointer-events-none z-5"
          style={{ left: rangeLeft, width: rangeWidth }} />
      )}

      {hoverState && hoverState.type === 'split' && !isDraggingRange.current && (
        <button className="pointer-split-btn timeline-arch-button absolute bottom-0 z-10 pointer-events-auto flex items-center justify-center"
          data-tone="accent" style={{ left: hoverState.x - 8 }}
          onPointerDown={(e) => { e.stopPropagation(); onPointerClick(hoverState.seg.id, hoverState.time); setHoverState(null); }}>
          <Scissors className="w-2 h-2" />
        </button>
      )}
      {hoverState && hoverState.type === 'add' && onAddPointerSegment && !isDraggingRange.current && (
        <button className="pointer-add-btn timeline-arch-button absolute bottom-0 z-10 pointer-events-auto flex items-center justify-center text-[8px] font-bold"
          data-tone="warning" style={{ left: hoverState.x - 8 }}
          onPointerDown={(e) => {
            e.stopPropagation();
            const rect = e.currentTarget.parentElement!.getBoundingClientRect();
            const time = (hoverState.x / rect.width) * safeDuration;
            onAddPointerSegment(time); setHoverState(null);
          }}>
          +
        </button>
      )}
    </div>
  );
};
