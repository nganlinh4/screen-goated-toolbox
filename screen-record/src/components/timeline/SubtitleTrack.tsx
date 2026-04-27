import React, { useRef, useState } from 'react';
import { Scissors } from 'lucide-react';
import { SubtitleSegment, VideoSegment } from '@/types/video';
import type {
  SubtitleGenerationIndicator,
} from '@/lib/subtitleGenerationPlan';
import type { TrackSelectionRange } from '@/lib/timelineSegmentSelection';
import { buildTextSplitPreview } from '@/lib/textSplitPreview';
import {
  getHandlePriorityThresholdTime,
  isTimeNearRangeBoundary,
} from './trackHoverUtils';
import { getVisibleSubtitleSegments } from '@/lib/subtitleTracks';
import { useTrackRangeSelect } from './useTrackRangeSelect';

interface SubtitleTrackProps {
  segment: VideoSegment;
  duration: number;
  editingSubtitleId: string | null;
  onSubtitleClick: (id: string) => void;
  onSubtitleSplit?: (id: string, splitTime: number) => void;
  onSubtitleDuplicate?: (id: string) => void;
  onHandleDragStart: (id: string, type: 'start' | 'end' | 'body', offset?: number) => void;
  onAddSubtitle?: (atTime?: number) => void;
  onDeleteSubtitleSegments?: (ids: string[]) => void;
  onSelectionChange?: (ids: string[]) => void;
  onRangeChange?: (range: TrackSelectionRange | null) => void;
  clearSignal?: number;
  generationIndicator?: SubtitleGenerationIndicator | null;
  translationChunkPreview?: {
    groups: Record<string, number>;
    groupCount: number;
  } | null;
}

const TRANSLATION_CHUNK_COLORS = [
  '#2563eb',
  '#0f9f8d',
  '#d97706',
  '#8b5cf6',
  '#e11d48',
  '#0891b2',
  '#65a30d',
  '#f97316',
];

export const SubtitleTrack: React.FC<SubtitleTrackProps> = ({
  segment,
  duration,
  editingSubtitleId,
  onSubtitleClick,
  onSubtitleSplit,
  onSubtitleDuplicate,
  onHandleDragStart,
  onAddSubtitle,
  onDeleteSubtitleSegments,
  onSelectionChange,
  onRangeChange,
  clearSignal,
  generationIndicator,
  translationChunkPreview,
}) => {
  const [hoverState, setHoverState] = useState<
    | { type: 'split'; x: number; time: number; seg: SubtitleSegment; preview: { leftText: string; rightText: string } | null }
    | { type: 'add'; x: number }
    | null
  >(null);

  const safeDuration = Math.max(duration, 0.001);
  const subtitles = getVisibleSubtitleSegments(segment);
  const lastClickRef = useRef<{ id: string | null; time: number }>({ id: null, time: 0 });
  const DOUBLE_CLICK_MS = 350;

  const {
    selectedIds, selectedRange, rangeSelect, activeDragMode, trackRef, isDraggingRange,
    onSegmentPointerDown,
    addSegmentSelection,
    handleTrackPointerDown, handleTrackPointerMove, handleTrackPointerUp,
  } = useTrackRangeSelect(
    subtitles,
    duration,
    onSelectionChange,
    onRangeChange,
    onDeleteSubtitleSegments,
    clearSignal,
    {
      allowCtrlDragAnywhere: true,
    },
  );

  const handleMouseMove = (e: React.MouseEvent<HTMLDivElement>) => {
    if (isDraggingRange.current) return;
    const rect = e.currentTarget.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const time = (x / rect.width) * safeDuration;
    const thresholdTime = getHandlePriorityThresholdTime(safeDuration, rect.width);

    const containing = subtitles.find(seg => time >= seg.startTime && time <= seg.endTime);
    if (containing) {
      const preview = buildTextSplitPreview({
        text: containing.text,
        startTime: containing.startTime,
        endTime: containing.endTime,
        splitTime: time,
      });
      const canSplit = onSubtitleSplit && preview && time > containing.startTime + 0.15 && time < containing.endTime - 0.15;
      setHoverState(
        canSplit
          ? {
              type: 'split',
              x,
              time,
              seg: containing,
              preview,
            }
          : null,
      );
      return;
    }
    if (isTimeNearRangeBoundary(time, subtitles, thresholdTime)) {
      setHoverState(null);
      return;
    }
    setHoverState(onAddSubtitle ? { type: 'add', x } : null);
  };

  const rangeLeft = rangeSelect ? Math.min(rangeSelect.startX, rangeSelect.endX) : 0;
  const rangeWidth = rangeSelect ? Math.abs(rangeSelect.endX - rangeSelect.startX) : 0;
  const selectedRangeLeft = selectedRange
    ? `${(Math.min(selectedRange.startTime, selectedRange.endTime) / safeDuration) * 100}%`
    : '0%';
  const selectedRangeWidth = selectedRange
    ? `${((Math.max(selectedRange.endTime, selectedRange.startTime) - Math.min(selectedRange.startTime, selectedRange.endTime)) / safeDuration) * 100}%`
    : '0%';
  const indicatorLeft = generationIndicator?.mode === 'range' && generationIndicator.range
    ? `${(Math.min(generationIndicator.range.startTime, generationIndicator.range.endTime) / safeDuration) * 100}%`
    : '0%';
  const indicatorWidth = generationIndicator?.mode === 'range' && generationIndicator.range
    ? `${(Math.max(generationIndicator.range.endTime, generationIndicator.range.startTime) - Math.min(generationIndicator.range.startTime, generationIndicator.range.endTime)) / safeDuration * 100}%`
    : '100%';
  const rangePillClassName = "pointer-events-none absolute inset-y-0 overflow-hidden rounded-md border border-[color:color-mix(in_srgb,var(--primary-color)_58%,transparent)] bg-[color:color-mix(in_srgb,var(--primary-color)_18%,transparent)]";

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
      {generationIndicator && (
        <div
          className="subtitle-generation-indicator pointer-events-none absolute inset-y-0 z-[1] overflow-hidden rounded-md border border-[color:color-mix(in_srgb,var(--timeline-zoom-color)_50%,transparent)] bg-[color:color-mix(in_srgb,var(--timeline-zoom-color)_18%,transparent)]"
          style={{
            left: indicatorLeft,
            width: indicatorWidth,
          }}
        >
          <div className="subtitle-generation-indicator-pulse absolute inset-0 animate-pulse bg-[linear-gradient(90deg,transparent_0%,color-mix(in_srgb,var(--timeline-zoom-color)_30%,transparent)_50%,transparent_100%)]" />
        </div>
      )}

      {selectedRange && (
        <div
          className={`subtitle-selected-range ${rangePillClassName} z-[2]`}
          style={{
            left: selectedRangeLeft,
            width: selectedRangeWidth,
          }}
        />
      )}

      {subtitles.map((subtitle) => {
        const chunkIndex = translationChunkPreview?.groups[subtitle.id];
        const chunkColor = typeof chunkIndex === 'number'
          ? TRANSLATION_CHUNK_COLORS[chunkIndex % TRANSLATION_CHUNK_COLORS.length]
          : null;
        return (
        <div
          key={subtitle.id}
          onPointerDown={(e) => {
            if (e.shiftKey || e.ctrlKey) return;
            const now = performance.now();
            const last = lastClickRef.current;
            const isDouble =
              !!onSubtitleDuplicate
              && last.id === subtitle.id
              && now - last.time < DOUBLE_CLICK_MS;
            if (isDouble) {
              e.stopPropagation();
              e.preventDefault();
              lastClickRef.current = { id: null, time: 0 };
              onSubtitleDuplicate?.(subtitle.id);
              return;
            }
            lastClickRef.current = { id: subtitle.id, time: now };
            const preserveGroupDrag = selectedIds.has(subtitle.id) && selectedIds.size > 1;
            if (!preserveGroupDrag) {
              addSegmentSelection(subtitle.id);
            }
            e.stopPropagation();
            onSegmentPointerDown();
            const rect = e.currentTarget.parentElement!.getBoundingClientRect();
            const clickX = e.clientX - rect.left;
            const clickTime = (clickX / rect.width) * safeDuration;
            onHandleDragStart(subtitle.id, 'body', clickTime - subtitle.startTime);
          }}
          onClick={(e) => {
            if (e.ctrlKey) return;
            e.stopPropagation();
            if (e.shiftKey) {
              addSegmentSelection(subtitle.id, { shiftKey: true });
            }
            onSubtitleClick(subtitle.id);
          }}
          className="subtitle-segment timeline-block absolute h-full cursor-move group"
          data-tone="accent"
          data-active={editingSubtitleId === subtitle.id ? 'true' : 'false'}
          data-selected={selectedIds.has(subtitle.id) ? 'true' : undefined}
          style={{
            left: `${(subtitle.startTime / safeDuration) * 100}%`,
            width: `${((subtitle.endTime - subtitle.startTime) / safeDuration) * 100}%`,
            ...(chunkColor
              ? {
                  background: `color-mix(in srgb, ${chunkColor} 28%, var(--ui-surface-3))`,
                  borderColor: `color-mix(in srgb, ${chunkColor} 62%, var(--timeline-lane-border))`,
                  boxShadow: `0 0 0 1px color-mix(in srgb, ${chunkColor} 38%, transparent), 0 0 10px color-mix(in srgb, ${chunkColor} 24%, transparent)`,
                }
              : {}),
          }}
        >
          <div className="subtitle-segment-content absolute inset-0 flex items-center justify-center overflow-hidden px-1">
            <span className="truncate text-[10px] text-[var(--on-surface)]">
              {subtitle.text}
            </span>
          </div>
          <div className="subtitle-handle-start absolute inset-y-0 -left-[2px] w-[5px] cursor-ew-resize flex items-center justify-center opacity-0 group-hover:opacity-100 z-10"
            onPointerDown={(e) => {
              if (e.ctrlKey) return;
              e.stopPropagation();
              onHandleDragStart(subtitle.id, 'start');
            }}>
            <div className="subtitle-handle-bar timeline-handle-pill" />
          </div>
          <div className="subtitle-handle-end absolute inset-y-0 -right-[2px] w-[5px] cursor-ew-resize flex items-center justify-center opacity-0 group-hover:opacity-100 z-10"
            onPointerDown={(e) => {
              if (e.ctrlKey) return;
              e.stopPropagation();
              onHandleDragStart(subtitle.id, 'end');
            }}>
            <div className="subtitle-handle-bar timeline-handle-pill" />
          </div>
        </div>
      );
      })}

      {rangeSelect && rangeWidth > 2 && activeDragMode === 'ctrl-range' && (
        <div className={`subtitle-time-range-drawer ${rangePillClassName} z-[6]`}
          style={{ left: rangeLeft, width: rangeWidth }} />
      )}

      {rangeSelect && rangeWidth > 2 && activeDragMode !== 'ctrl-range' && (
        <div
          className="subtitle-range-select timeline-range-select absolute pointer-events-none z-5"
          style={{ left: rangeLeft, width: rangeWidth }}
        />
      )}

      {hoverState?.type === 'split' && !isDraggingRange.current && (
        <div className="subtitle-split-control absolute bottom-0 z-10 pointer-events-auto" style={{ left: hoverState.x - 8 }}>
          <div className="subtitle-split-hover group/subtitle-split relative">
            <div className="subtitle-split-preview-chip timeline-chip absolute left-1/2 z-30 -translate-x-1/2 bottom-[calc(100%+6px)] px-2.5 py-1 text-[11px] font-semibold whitespace-nowrap pointer-events-none opacity-0 translate-y-1 transition-all duration-150 group-hover/subtitle-split:opacity-100 group-hover/subtitle-split:translate-y-0" data-tone="accent">
              <span>{hoverState.preview?.leftText ?? hoverState.seg.text}</span>
              <span className="mx-1 opacity-80">|</span>
              <span>{hoverState.preview?.rightText ?? hoverState.seg.text}</span>
            </div>
            <button className="subtitle-split-btn timeline-arch-button flex items-center justify-center"
              data-tone="accent"
              onPointerDown={(e) => { e.stopPropagation(); onSubtitleSplit?.(hoverState.seg.id, hoverState.time); setHoverState(null); }}>
              <Scissors className="w-2 h-2" />
            </button>
          </div>
        </div>
      )}

      {hoverState?.type === 'add' && onAddSubtitle && !isDraggingRange.current && (
        <button className="subtitle-add-btn timeline-arch-button absolute bottom-0 z-10 pointer-events-auto flex items-center justify-center text-[8px] font-bold"
          data-tone="accent" style={{ left: hoverState.x - 8 }}
          onPointerDown={(e) => {
            e.stopPropagation();
            const rect = e.currentTarget.parentElement!.getBoundingClientRect();
            const time = (hoverState.x / rect.width) * safeDuration;
            onAddSubtitle(time);
            setHoverState(null);
          }}>
          +
        </button>
      )}
    </div>
  );
};
