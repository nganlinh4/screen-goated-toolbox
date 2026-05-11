import React, { useMemo, useRef, useState } from 'react';
import type { CSSProperties } from 'react';
import { Scissors } from 'lucide-react';
import { ImportedAudioSegment, SubtitleSegment, SubtitleSourceGroup, VideoSegment } from '@/types/video';
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
import {
  getSubtitleSourceGroup,
  getSubtitleSourceGroupColor,
  getSubtitleSourceGroupId,
  makeSubtitleSourceGroup,
} from '@/lib/subtitleSourceGroups';
import { countFrontendRender } from '@/lib/frontendPerfDiagnostics';
import {
  SegmentBlocksCanvas,
  type TimelineVisibleRange,
} from './SegmentBlocksCanvas';
import { buildTimelineRenderWindow } from './timelineSegmentIndex';

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
  onEmptyClick?: (time: number) => void;
  generationIndicator?: SubtitleGenerationIndicator | null;
  translationChunkPreview?: {
    groups: Record<string, number>;
    groupCount: number;
  } | null;
  audioSegments?: ImportedAudioSegment[];
  isDeviceAudioAvailable?: boolean;
  isMicAudioAvailable?: boolean;
  onAssignSubtitleSourceGroup?: (ids: string[], sourceGroup: SubtitleSourceGroup) => void;
  canvasWidthPx?: number;
  visibleTimeRange?: TimelineVisibleRange | null;
}

const DENSE_SUBTITLE_COUNT = 260;
const MIN_INTERACTIVE_SUBTITLE_PX = 7;
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

function rangesOverlap(
  a: { startTime: number; endTime: number },
  b: { startTime: number; endTime: number },
) {
  return a.startTime < b.endTime && b.startTime < a.endTime;
}

function getOverlapRange(
  segment: { startTime: number; endTime: number },
  elevated: { startTime: number; endTime: number } | null,
) {
  if (!elevated || !rangesOverlap(segment, elevated)) return null;
  const start = Math.max(segment.startTime, elevated.startTime);
  const end = Math.min(segment.endTime, elevated.endTime);
  const duration = Math.max(segment.endTime - segment.startTime, 0.0001);
  return {
    startPct: ((start - segment.startTime) / duration) * 100,
    endPct: ((end - segment.startTime) / duration) * 100,
  };
}

function buildContentMaskStyle(
  ranges: Array<{ startPct: number; endPct: number }>,
): CSSProperties | undefined {
  if (ranges.length === 0) return undefined;
  const merged = [...ranges]
    .sort((a, b) => a.startPct - b.startPct)
    .reduce<Array<{ startPct: number; endPct: number }>>((acc, range) => {
      const startPct = Math.max(0, Math.min(100, range.startPct));
      const endPct = Math.max(startPct, Math.min(100, range.endPct));
      const last = acc[acc.length - 1];
      if (last && startPct <= last.endPct) {
        last.endPct = Math.max(last.endPct, endPct);
      } else if (endPct > startPct) {
        acc.push({ startPct, endPct });
      }
      return acc;
    }, []);
  if (merged.length === 0) return undefined;
  const stops: string[] = ["black 0%"];
  for (const range of merged) {
    stops.push(`black ${range.startPct}%`);
    stops.push(`transparent ${range.startPct}%`);
    stops.push(`transparent ${range.endPct}%`);
    stops.push(`black ${range.endPct}%`);
  }
  stops.push("black 100%");
  const maskImage = `linear-gradient(to right, ${stops.join(", ")})`;
  return { maskImage, WebkitMaskImage: maskImage };
}

function isSegmentStackedAbove(
  index: number,
  rank: number,
  otherIndex: number,
  otherRank: number,
) {
  return otherRank > rank || (otherRank === rank && otherIndex > index);
}

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
  onEmptyClick,
  generationIndicator,
  translationChunkPreview,
  audioSegments = [],
  isDeviceAudioAvailable = false,
  isMicAudioAvailable = false,
  onAssignSubtitleSourceGroup,
  canvasWidthPx = 0,
  visibleTimeRange,
}) => {
  countFrontendRender('SubtitleTrack');
  const [hoverState, setHoverState] = useState<
    | { type: 'split'; x: number; time: number; seg: SubtitleSegment; preview: { leftText: string; rightText: string } | null }
    | { type: 'add'; x: number }
    | null
  >(null);
  const [sourceMenu, setSourceMenu] = useState<{
    x: number;
    y: number;
    subtitleId: string;
  } | null>(null);
  const [frontSubtitleId, setFrontSubtitleId] = useState<string | null>(null);

  const safeDuration = Math.max(duration, 0.001);
  const subtitles = getVisibleSubtitleSegments(segment);
  const denseMode = subtitles.length >= DENSE_SUBTITLE_COUNT;
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
      onEmptyClick,
    },
  );

  const renderWindow = useMemo(
    () => buildTimelineRenderWindow({
      segments: subtitles,
      duration,
      canvasWidthPx,
      visibleRange: visibleTimeRange,
      denseMode,
      selectedIds,
      activeIds: editingSubtitleId ? new Set([editingSubtitleId]) : undefined,
      minInteractivePx: MIN_INTERACTIVE_SUBTITLE_PX,
    }),
    [canvasWidthPx, denseMode, duration, editingSubtitleId, selectedIds, subtitles, visibleTimeRange],
  );

  const findSubtitleAtTime = (time: number) => renderWindow.index.hitTest(time);

  const canvasSegments = useMemo(
    () =>
      renderWindow.canvasSegments.map((subtitle) => {
        const chunkIndex = translationChunkPreview?.groups[subtitle.id];
        const chunkColor = typeof chunkIndex === 'number'
          ? TRANSLATION_CHUNK_COLORS[chunkIndex % TRANSLATION_CHUNK_COLORS.length]
          : null;
        const sourceColor = getSubtitleSourceGroupColor(getSubtitleSourceGroupId(subtitle));
        return {
          id: subtitle.id,
          startTime: subtitle.startTime,
          endTime: subtitle.endTime,
          color: chunkColor ?? sourceColor,
          selected: selectedIds.has(subtitle.id) || editingSubtitleId === subtitle.id,
        };
      }),
    [editingSubtitleId, renderWindow.canvasSegments, selectedIds, translationChunkPreview?.groups],
  );

  const handleTrackPointerDownFast = (e: React.PointerEvent<HTMLDivElement>) => {
    if (e.ctrlKey || !denseMode) {
      handleTrackPointerDown(e);
      return;
    }
    const rect = e.currentTarget.getBoundingClientRect();
    const time = ((e.clientX - rect.left) / Math.max(rect.width, 1)) * safeDuration;
    const hit = findSubtitleAtTime(time);
    if (!hit) {
      onEmptyClick?.(time);
      return;
    }
    e.stopPropagation();
    if (e.shiftKey) {
      addSegmentSelection(hit.id, { shiftKey: true });
    } else if (!selectedIds.has(hit.id) || selectedIds.size <= 1) {
      addSegmentSelection(hit.id);
    }
    onSegmentPointerDown();
    onSubtitleClick(hit.id);
  };

  const handleMouseMove = (e: React.MouseEvent<HTMLDivElement>) => {
    if (isDraggingRange.current) return;
    if (denseMode) {
      if (hoverState !== null) setHoverState(null);
      return;
    }
    const rect = e.currentTarget.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const time = (x / rect.width) * safeDuration;
    const thresholdTime = getHandlePriorityThresholdTime(safeDuration, rect.width);

    const containing = renderWindow.index.hitTest(time);
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
  const assignSourceGroup = (sourceGroup: SubtitleSourceGroup) => {
    if (!sourceMenu || !onAssignSubtitleSourceGroup) return;
    const targetIds = selectedIds.has(sourceMenu.subtitleId) && selectedIds.size > 1
      ? [...selectedIds]
      : [sourceMenu.subtitleId];
    onAssignSubtitleSourceGroup(targetIds, sourceGroup);
    setSourceMenu(null);
  };

  return (
    <div
      ref={trackRef}
      className="subtitle-track timeline-lane relative h-7"
      onMouseMove={handleMouseMove}
      onMouseLeave={() => { if (!isDraggingRange.current) setHoverState(null); }}
      onPointerDown={handleTrackPointerDownFast}
      onPointerMove={handleTrackPointerMove}
      onPointerUp={handleTrackPointerUp}
    >
      {denseMode && (
        <SegmentBlocksCanvas
          segments={canvasSegments}
          duration={duration}
          visibleRange={visibleTimeRange}
          colorVar="--timeline-zoom-color"
          fallbackColor="#22d3ee"
          alpha={0.38}
        />
      )}

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

      {renderWindow.domSegments.map((subtitle, subtitleIndex) => {
        const isSelected = selectedIds.has(subtitle.id);
        const isActive = editingSubtitleId === subtitle.id;
        const chunkIndex = translationChunkPreview?.groups[subtitle.id];
        const chunkColor = typeof chunkIndex === 'number'
          ? TRANSLATION_CHUNK_COLORS[chunkIndex % TRANSLATION_CHUNK_COLORS.length]
          : null;
        const sourceGroupId = getSubtitleSourceGroupId(subtitle);
        const sourceColor = getSubtitleSourceGroupColor(sourceGroupId);
        const isUnassignedSource = getSubtitleSourceGroup(subtitle).kind === 'unassigned';
        const accentColor = chunkColor ?? sourceColor;
        const isFront = frontSubtitleId === subtitle.id || isActive;
        const stackRank = isFront ? 3 : isSelected ? 2 : 1;
        const hasCoveredSegmentUnderneath = renderWindow.domSegments.some((other, otherIndex) => {
          if (other.id === subtitle.id || !rangesOverlap(subtitle, other)) return false;
          const otherIsFront = frontSubtitleId === other.id || editingSubtitleId === other.id;
          const otherRank = otherIsFront ? 3 : selectedIds.has(other.id) ? 2 : 1;
          return !isSegmentStackedAbove(subtitleIndex, stackRank, otherIndex, otherRank);
        });
        const overlapRanges = renderWindow.domSegments
          .map((other, otherIndex) => {
            if (other.id === subtitle.id) return null;
            const otherIsFront = frontSubtitleId === other.id || editingSubtitleId === other.id;
            const otherRank = otherIsFront ? 3 : selectedIds.has(other.id) ? 2 : 1;
            if (!isSegmentStackedAbove(subtitleIndex, stackRank, otherIndex, otherRank)) return null;
            return getOverlapRange(subtitle, other);
          })
          .filter((range): range is NonNullable<ReturnType<typeof getOverlapRange>> => Boolean(range));
        const contentMaskStyle = buildContentMaskStyle(overlapRanges);
        const fillStrength = hasCoveredSegmentUnderneath ? 34 : isSelected ? 22 : 28;
        return (
        <div
          key={subtitle.id}
          onContextMenu={(e) => {
            if (!onAssignSubtitleSourceGroup) return;
            e.preventDefault();
            e.stopPropagation();
            if (!selectedIds.has(subtitle.id)) {
              addSegmentSelection(subtitle.id);
            }
            setFrontSubtitleId(subtitle.id);
            setSourceMenu({
              x: e.clientX,
              y: e.clientY,
              subtitleId: subtitle.id,
            });
          }}
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
            setFrontSubtitleId(subtitle.id);
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
            setFrontSubtitleId(subtitle.id);
            if (e.shiftKey) {
              addSegmentSelection(subtitle.id, { shiftKey: true });
            }
            onSubtitleClick(subtitle.id);
          }}
          className="subtitle-segment timeline-block absolute h-full cursor-move overflow-hidden group"
          data-tone="accent"
          data-active={isActive ? 'true' : 'false'}
          data-selected={isSelected ? 'true' : undefined}
          data-source-unassigned={isUnassignedSource ? 'true' : undefined}
          style={{
            left: `${(subtitle.startTime / safeDuration) * 100}%`,
            width: `${((subtitle.endTime - subtitle.startTime) / safeDuration) * 100}%`,
            zIndex: isFront ? 5 : isSelected ? 4 : 3,
            ...(!accentColor && hasCoveredSegmentUnderneath
              ? {
                  background:
                    "color-mix(in srgb, var(--timeline-zoom-color) 34%, transparent)",
                }
              : {}),
            ...(accentColor
              ? {
                  background: hasCoveredSegmentUnderneath
                    ? `color-mix(in srgb, ${accentColor} ${fillStrength}%, transparent)`
                    : `color-mix(in srgb, ${accentColor} ${fillStrength}%, var(--ui-surface-3))`,
                  borderColor: `color-mix(in srgb, ${accentColor} 62%, var(--timeline-lane-border))`,
                  boxShadow: `0 0 0 1px color-mix(in srgb, ${accentColor} 38%, transparent), 0 0 10px color-mix(in srgb, ${accentColor} 24%, transparent)`,
                }
              : {}),
            ...(isUnassignedSource
              ? {
                  borderStyle: 'dashed',
                  borderColor: 'color-mix(in srgb, var(--on-surface-variant) 58%, var(--timeline-lane-border))',
                }
              : {}),
          }}
        >
          <div
            className="subtitle-segment-content absolute inset-0 z-[1] flex min-w-0 items-center justify-center overflow-hidden px-1"
            style={contentMaskStyle}
          >
            <span className="min-w-0 max-w-full truncate text-[10px] text-[var(--on-surface)]">
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

      {sourceMenu && (
        <div
          className="subtitle-source-menu ui-surface-elevated fixed z-[110] min-w-[190px] rounded-lg p-1.5 text-[11px]"
          style={{ left: sourceMenu.x, top: sourceMenu.y }}
          onPointerDown={(event) => event.stopPropagation()}
          onMouseLeave={() => setSourceMenu(null)}
        >
          <button
            type="button"
            className="subtitle-source-menu-item flex w-full items-center rounded-md px-2 py-1.5 text-left text-[var(--on-surface-variant)] hover:bg-[var(--ui-surface-3)] hover:text-[var(--on-surface)]"
            onClick={() => assignSourceGroup({ kind: 'unassigned', assignment: 'manual' })}
          >
            Unassigned
          </button>
          {isDeviceAudioAvailable && (
            <button
              type="button"
              className="subtitle-source-menu-item flex w-full items-center rounded-md px-2 py-1.5 text-left text-[var(--on-surface-variant)] hover:bg-[var(--ui-surface-3)] hover:text-[var(--on-surface)]"
              onClick={() => assignSourceGroup({ kind: 'video', assignment: 'manual' })}
            >
              Device audio
            </button>
          )}
          {isMicAudioAvailable && (
            <button
              type="button"
              className="subtitle-source-menu-item flex w-full items-center rounded-md px-2 py-1.5 text-left text-[var(--on-surface-variant)] hover:bg-[var(--ui-surface-3)] hover:text-[var(--on-surface)]"
              onClick={() => assignSourceGroup({ kind: 'mic', assignment: 'manual' })}
            >
              Microphone
            </button>
          )}
          {audioSegments.map((audioSegment) => (
            <button
              key={audioSegment.id}
              type="button"
              className="subtitle-source-menu-item flex w-full items-center rounded-md px-2 py-1.5 text-left text-[var(--on-surface-variant)] hover:bg-[var(--ui-surface-3)] hover:text-[var(--on-surface)]"
              onClick={() => assignSourceGroup(makeSubtitleSourceGroup({
                kind: 'audio',
                assignment: 'manual',
                audioSegment,
              }))}
            >
              {audioSegment.name || 'Audio'}
            </button>
          ))}
        </div>
      )}

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
