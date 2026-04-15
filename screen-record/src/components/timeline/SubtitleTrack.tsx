import React, { useState } from 'react';
import { Scissors } from 'lucide-react';
import { SubtitleSegment, VideoSegment } from '@/types/video';
import {
  getHandlePriorityThresholdTime,
  isTimeNearRangeBoundary,
} from './trackHoverUtils';
import { useTrackRangeSelect } from './useTrackRangeSelect';

interface SubtitleTrackProps {
  segment: VideoSegment;
  duration: number;
  editingSubtitleId: string | null;
  onSubtitleClick: (id: string) => void;
  onSubtitleSplit?: (id: string, splitTime: number) => void;
  onHandleDragStart: (id: string, type: 'start' | 'end' | 'body', offset?: number) => void;
  onDeleteSubtitleSegments?: (ids: string[]) => void;
  onSelectionChange?: (ids: string[]) => void;
  clearSignal?: number;
}

export const SubtitleTrack: React.FC<SubtitleTrackProps> = ({
  segment,
  duration,
  editingSubtitleId,
  onSubtitleClick,
  onSubtitleSplit,
  onHandleDragStart,
  onDeleteSubtitleSegments,
  onSelectionChange,
  clearSignal,
}) => {
  const [hoverState, setHoverState] = useState<
    | { type: 'split'; x: number; time: number; seg: SubtitleSegment }
    | null
  >(null);

  const safeDuration = Math.max(duration, 0.001);
  const subtitles = segment.subtitleSegments ?? [];

  const {
    selectedIds, rangeSelect, trackRef, isDraggingRange,
    onSegmentPointerDown,
    handleTrackPointerDown, handleTrackPointerMove, handleTrackPointerUp,
  } = useTrackRangeSelect(subtitles, duration, onSelectionChange, onDeleteSubtitleSegments, clearSignal);

  const handleMouseMove = (e: React.MouseEvent<HTMLDivElement>) => {
    if (isDraggingRange.current) return;
    const rect = e.currentTarget.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const time = (x / rect.width) * safeDuration;
    const thresholdTime = getHandlePriorityThresholdTime(safeDuration, rect.width);

    const containing = subtitles.find(seg => time >= seg.startTime && time <= seg.endTime);
    if (containing) {
      const canSplit = onSubtitleSplit && time > containing.startTime + 0.15 && time < containing.endTime - 0.15;
      setHoverState(canSplit ? { type: 'split', x, time, seg: containing } : null);
      return;
    }
    if (isTimeNearRangeBoundary(time, subtitles, thresholdTime)) {
      setHoverState(null);
      return;
    }
    setHoverState(null);
  };

  const rangeLeft = rangeSelect ? Math.min(rangeSelect.startX, rangeSelect.endX) : 0;
  const rangeWidth = rangeSelect ? Math.abs(rangeSelect.endX - rangeSelect.startX) : 0;

  return (
    <div
      ref={trackRef}
      className="subtitle-track timeline-lane relative h-7"
      onMouseMove={handleMouseMove}
      onMouseLeave={() => { if (!isDraggingRange.current) setHoverState(null); }}
      onPointerDown={handleTrackPointerDown}
      onPointerMove={handleTrackPointerMove}
      onPointerUp={handleTrackPointerUp}
    >
      {subtitles.map((subtitle) => (
        <div
          key={subtitle.id}
          onPointerDown={(e) => {
            e.stopPropagation();
            onSegmentPointerDown();
            const rect = e.currentTarget.parentElement!.getBoundingClientRect();
            const clickX = e.clientX - rect.left;
            const clickTime = (clickX / rect.width) * safeDuration;
            onHandleDragStart(subtitle.id, 'body', clickTime - subtitle.startTime);
          }}
          onClick={(e) => { e.stopPropagation(); onSubtitleClick(subtitle.id); }}
          className="subtitle-segment timeline-block absolute h-full cursor-move group"
          data-tone="accent"
          data-active={editingSubtitleId === subtitle.id ? 'true' : 'false'}
          data-selected={selectedIds.has(subtitle.id) ? 'true' : undefined}
          style={{
            left: `${(subtitle.startTime / safeDuration) * 100}%`,
            width: `${((subtitle.endTime - subtitle.startTime) / safeDuration) * 100}%`,
          }}
        >
          <div className="subtitle-segment-content absolute inset-0 flex items-center justify-center overflow-hidden px-1">
            <span className="truncate text-[10px] text-[var(--on-surface)]">
              {subtitle.text}
            </span>
          </div>
          <div className="subtitle-handle-start absolute inset-y-0 -left-[2px] w-[5px] cursor-ew-resize flex items-center justify-center opacity-0 group-hover:opacity-100 z-10"
            onPointerDown={(e) => { e.stopPropagation(); onHandleDragStart(subtitle.id, 'start'); }}>
            <div className="subtitle-handle-bar timeline-handle-pill" />
          </div>
          <div className="subtitle-handle-end absolute inset-y-0 -right-[2px] w-[5px] cursor-ew-resize flex items-center justify-center opacity-0 group-hover:opacity-100 z-10"
            onPointerDown={(e) => { e.stopPropagation(); onHandleDragStart(subtitle.id, 'end'); }}>
            <div className="subtitle-handle-bar timeline-handle-pill" />
          </div>
        </div>
      ))}

      {rangeSelect && rangeWidth > 2 && (
        <div className="subtitle-range-select timeline-range-select absolute pointer-events-none z-5"
          style={{ left: rangeLeft, width: rangeWidth }} />
      )}

      {hoverState?.type === 'split' && !isDraggingRange.current && (
        <button className="subtitle-split-btn timeline-arch-button absolute bottom-0 z-10 pointer-events-auto flex items-center justify-center"
          data-tone="accent" style={{ left: hoverState.x - 8 }}
          onPointerDown={(e) => { e.stopPropagation(); onSubtitleSplit?.(hoverState.seg.id, hoverState.time); setHoverState(null); }}>
          <Scissors className="w-2 h-2" />
        </button>
      )}
    </div>
  );
};
