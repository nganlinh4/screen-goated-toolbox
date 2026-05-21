import React, { useMemo, useState } from "react";
import type { CSSProperties } from "react";
import type { AudioGainPoint, NarrationSegment } from "@/types/video";
import type { TrackSelectionRange } from "@/lib/timelineSegmentSelection";
import { useTrackRangeSelect } from "./useTrackRangeSelect";
import { TrackVolumeCurve } from "./TrackVolumeCurve";
import { useAudioSegmentTimelineEdit } from "./useAudioSegmentTimelineEdit";
import { AudioWaveformLayer } from "./AudioWaveformLayer";
import {
  SegmentBlocksCanvas,
  type TimelineVisibleRange,
} from "./SegmentBlocksCanvas";
import { buildTimelineRenderWindow } from "./timelineSegmentIndex";
import {
  mergeLiveNarrationSegments,
  useLiveNarrationState,
} from "@/lib/liveNarrationStreamStore";
import { countFrontendRender } from "@/lib/frontendPerfDiagnostics";

interface NarrationTrackProps {
  segments: NarrationSegment[];
  liveProjectId?: string | null;
  duration: number;
  onSegmentClick?: (id: string) => void;
  onDeleteSegments?: (ids: string[]) => void;
  selectedIds?: ReadonlySet<string>;
  selectedRange?: TrackSelectionRange | null;
  onSelectionChange?: (ids: string[]) => void;
  onRangeChange?: (range: TrackSelectionRange | null) => void;
  onUpdateSegment?: (id: string, patch: Partial<NarrationSegment>) => void;
  viewMode?: "compact" | "volume";
  clearSignal?: number;
  onEmptyClick?: (time: number) => void;
  /** Track-global volume envelope (project-relative seconds). */
  volumePoints?: AudioGainPoint[];
  onUpdateVolumePoints?: (points: AudioGainPoint[]) => void;
  beginBatch?: () => void;
  commitBatch?: () => void;
  onCommitSegments?: () => void;
  canvasWidthPx?: number;
  visibleTimeRange?: TimelineVisibleRange | null;
}

const MIN_VISIBLE_SEC = 0.05;
const DENSE_SEGMENT_COUNT = 120;
const MIN_INTERACTIVE_SEGMENT_PX = 8;
const MIN_WAVEFORM_SEGMENT_PX = 28;

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

interface SelectableSegment {
  id: string;
  startTime: number;
  endTime: number;
}

export const NarrationTrack: React.FC<NarrationTrackProps> = ({
  segments,
  liveProjectId,
  duration,
  onSegmentClick,
  onDeleteSegments,
  selectedIds: externalSelectedIds,
  selectedRange: externalSelectedRange,
  onSelectionChange,
  onRangeChange,
  onUpdateSegment,
  viewMode = "compact",
  clearSignal,
  onEmptyClick,
  volumePoints,
  onUpdateVolumePoints,
  beginBatch,
  commitBatch,
  onCommitSegments,
  canvasWidthPx = 0,
  visibleTimeRange,
}) => {
  countFrontendRender("NarrationTrack");
  const safeDuration = Math.max(duration, 0.001);
  const [frontSegmentId, setFrontSegmentId] = useState<string | null>(null);
  const liveNarrationState = useLiveNarrationState(liveProjectId);
  const displayedSegments = useMemo(
    () => mergeLiveNarrationSegments(segments, liveNarrationState),
    [liveNarrationState, segments],
  );
  const segmentById = useMemo(
    () => new Map(displayedSegments.map((segment) => [segment.id, segment])),
    [displayedSegments],
  );

  const selectable = useMemo<SelectableSegment[]>(
    () =>
      displayedSegments.map((seg) => {
        const trimmed = Math.max(seg.outPoint - seg.inPoint, MIN_VISIBLE_SEC);
        const rate = seg.playbackRate && seg.playbackRate > 0 ? seg.playbackRate : 1;
        const visible = Math.max(trimmed / rate, MIN_VISIBLE_SEC);
        return {
          id: seg.id,
          startTime: seg.startTime,
          endTime: seg.startTime + visible,
        };
      }),
    [displayedSegments],
  );

  const {
    selectedIds,
    selectedRange,
    rangeSelect,
    activeDragMode,
    trackRef,
    addSegmentSelection,
    handleTrackPointerDown,
    handleTrackPointerMove,
    handleTrackPointerUp,
  } = useTrackRangeSelect(
    selectable,
    duration,
    onSelectionChange,
    onRangeChange,
    onDeleteSegments,
    clearSignal,
    { allowCtrlDragAnywhere: true, onEmptyClick },
  );

  const effectiveSelectedIds = externalSelectedIds ?? selectedIds;
  const effectiveSelectedRange = externalSelectedRange ?? selectedRange;
  const denseMode = displayedSegments.length >= DENSE_SEGMENT_COUNT;
  const renderWindow = useMemo(
    () => buildTimelineRenderWindow({
      segments: selectable,
      duration,
      canvasWidthPx,
      visibleRange: visibleTimeRange,
      denseMode,
      selectedIds: effectiveSelectedIds,
      minInteractivePx: MIN_INTERACTIVE_SEGMENT_PX,
    }),
    [canvasWidthPx, denseMode, duration, effectiveSelectedIds, selectable, visibleTimeRange],
  );
  const canvasSegments = useMemo(
    () =>
      renderWindow.canvasSegments.map((segment) => ({
        ...segment,
        selected: effectiveSelectedIds.has(segment.id),
      })),
    [effectiveSelectedIds, renderWindow.canvasSegments],
  );
  const {
    isDraggingAudioSegment,
    handleAudioSegmentPointerDown,
    handleAudioSegmentPointerMove,
    handleAudioSegmentPointerUp,
  } = useAudioSegmentTimelineEdit<NarrationSegment>({
    duration,
    onUpdateSegment,
    beginBatch,
    commitBatch,
    onCommitSegments,
  });

  const rangeLeft = rangeSelect ? Math.min(rangeSelect.startX, rangeSelect.endX) : 0;
  const rangeWidth = rangeSelect ? Math.abs(rangeSelect.endX - rangeSelect.startX) : 0;
  const selectedRangeLeftPct = effectiveSelectedRange
    ? `${(Math.min(effectiveSelectedRange.startTime, effectiveSelectedRange.endTime) / safeDuration) * 100}%`
    : "0%";
  const selectedRangeWidthPct = effectiveSelectedRange
    ? `${((Math.max(effectiveSelectedRange.endTime, effectiveSelectedRange.startTime) - Math.min(effectiveSelectedRange.startTime, effectiveSelectedRange.endTime)) / safeDuration) * 100}%`
    : "0%";
  const rangePillClassName =
    "narration-track-range-pill pointer-events-none absolute inset-y-0 overflow-hidden rounded-md border border-[color:color-mix(in_srgb,var(--secondary-color)_58%,transparent)] bg-[color:color-mix(in_srgb,var(--secondary-color)_18%,transparent)]";
  const getTrackVolumeAtTime = (_time: number, points: AudioGainPoint[] | undefined | null) => {
    if (!points || points.length === 0) return 1;
    const sorted = [...points].sort((a, b) => a.time - b.time);
    const idx = sorted.findIndex((point) => point.time >= _time);
    if (idx === -1) return sorted[sorted.length - 1]?.volume ?? 1;
    if (idx === 0) return sorted[0]?.volume ?? 1;
    const left = sorted[idx - 1];
    const right = sorted[idx];
    const ratio = Math.max(0, Math.min(1, (_time - left.time) / Math.max(0.0001, right.time - left.time)));
    const cosT = (1 - Math.cos(ratio * Math.PI)) / 2;
    return Math.max(0, Math.min(1, left.volume + (right.volume - left.volume) * cosT));
  };
  const findSegmentAtTime = (time: number) => {
    const hit = renderWindow.index.hitTest(time);
    return hit ? segmentById.get(hit.id) ?? null : null;
  };

  const handleTrackPointerDownFast = (e: React.PointerEvent<HTMLDivElement>) => {
    if (e.ctrlKey || !denseMode) {
      handleTrackPointerDown(e);
      return;
    }
    const rect = e.currentTarget.getBoundingClientRect();
    const time = ((e.clientX - rect.left) / Math.max(rect.width, 1)) * safeDuration;
    const hit = findSegmentAtTime(time);
    if (!hit) {
      onEmptyClick?.(time);
      return;
    }
    e.stopPropagation();
    if (e.shiftKey) {
      addSegmentSelection(hit.id, { shiftKey: true });
      return;
    }
    addSegmentSelection(hit.id);
    setFrontSegmentId(hit.id);
    onSegmentClick?.(hit.id);
  };

  return (
    <div
      ref={trackRef}
      className="narration-track timeline-lane relative h-7"
      onPointerDown={handleTrackPointerDownFast}
      onPointerMove={handleTrackPointerMove}
      onPointerUp={handleTrackPointerUp}
    >
      {denseMode && (
        <SegmentBlocksCanvas
          segments={canvasSegments}
          duration={duration}
          visibleRange={visibleTimeRange}
          colorVar="--secondary-color"
          fallbackColor="#a78bfa"
          alpha={0.34}
        />
      )}

      {effectiveSelectedRange && (
        <div
          className={`narration-track-selected-range ${rangePillClassName} z-[2]`}
          style={{ left: selectedRangeLeftPct, width: selectedRangeWidthPct }}
        />
      )}

      {renderWindow.domSegments.map((renderSegment, renderSegmentIndex) => {
        const seg = segmentById.get(renderSegment.id);
        if (!seg) return null;
        const trimmed = Math.max(seg.outPoint - seg.inPoint, MIN_VISIBLE_SEC);
        const rate = seg.playbackRate && seg.playbackRate > 0 ? seg.playbackRate : 1;
        const visible = Math.max(trimmed / rate, MIN_VISIBLE_SEC);
        const widthPct = Math.min(100, (visible / safeDuration) * 100);
        const leftPct = Math.min(100, Math.max(0, (seg.startTime / safeDuration) * 100));
        const isSelected = effectiveSelectedIds.has(seg.id);
        const isFront = frontSegmentId === seg.id;
        const stackRank = isFront ? 3 : isSelected ? 2 : 1;
        const hasCoveredSegmentUnderneath = renderWindow.domSegments.some((other, otherIndex) => {
          if (other.id === renderSegment.id || !rangesOverlap(renderSegment, other)) return false;
          const otherRank = frontSegmentId === other.id ? 3 : effectiveSelectedIds.has(other.id) ? 2 : 1;
          return !isSegmentStackedAbove(renderSegmentIndex, stackRank, otherIndex, otherRank);
        });
        const overlapRanges = renderWindow.domSegments
          .map((other, otherIndex) => {
            if (other.id === renderSegment.id) return null;
            const otherRank = frontSegmentId === other.id ? 3 : effectiveSelectedIds.has(other.id) ? 2 : 1;
            if (!isSegmentStackedAbove(renderSegmentIndex, stackRank, otherIndex, otherRank)) return null;
            return getOverlapRange(renderSegment, other);
          })
          .filter((range): range is NonNullable<ReturnType<typeof getOverlapRange>> => Boolean(range));
        const contentMaskStyle = buildContentMaskStyle(overlapRanges);
        const widthPx = (visible / safeDuration) * Math.max(canvasWidthPx, 1);
        const showSpeedBadge = Math.abs(rate - 1) > 0.001;
        const isEstimatedAlignment =
          seg.narrationAlignmentMode === "estimated" ||
          ((seg.narrationAlignmentConfidence ?? 1) < 0.6 && Boolean(seg.narrationGroupTakeId));
        const shouldRenderWaveform =
          viewMode === "volume" &&
          widthPx >= MIN_WAVEFORM_SEGMENT_PX &&
          (!denseMode || isSelected || renderWindow.canvasSegments.some((entry) => entry.id === seg.id));
        return (
          <div
            key={seg.id}
            className="narration-track-segment timeline-block absolute h-full cursor-move overflow-hidden group"
            data-narration-segment-id={seg.id}
            data-start-time={seg.startTime}
            data-in-point={seg.inPoint}
            data-out-point={seg.outPoint}
            data-playback-rate={rate}
            data-duration={seg.duration}
            data-timeline-duration={safeDuration}
            data-tone="secondary"
            data-selected={isSelected ? "true" : undefined}
            style={{
              left: `${leftPct}%`,
              width: `${widthPct}%`,
              zIndex: isFront ? 5 : isSelected ? 4 : 3,
              background:
                hasCoveredSegmentUnderneath
                  ? "color-mix(in srgb, var(--secondary-color) 34%, transparent)"
                  : isSelected
                    ? "color-mix(in srgb, var(--secondary-color) 18%, var(--ui-surface-3))"
                  : "color-mix(in srgb, var(--secondary-color) 22%, var(--ui-surface-3))",
              borderColor: isSelected
                ? "var(--secondary-color)"
                : "color-mix(in srgb, var(--secondary-color) 56%, var(--timeline-lane-border))",
              boxShadow: isSelected
                ? "0 0 0 1px var(--secondary-color), 0 0 10px color-mix(in srgb, var(--secondary-color) 28%, transparent)"
                : "0 0 0 1px color-mix(in srgb, var(--secondary-color) 32%, transparent)",
            }}
            onPointerDown={(e) => {
              if (e.ctrlKey) return;
              if (e.shiftKey) {
                e.stopPropagation();
                addSegmentSelection(seg.id, { shiftKey: true });
                return;
              }
              if (e.button !== 0) return;
              addSegmentSelection(seg.id);
              setFrontSegmentId(seg.id);
              onSegmentClick?.(seg.id);
              handleAudioSegmentPointerDown(e, seg);
            }}
            onPointerMove={handleAudioSegmentPointerMove}
            onPointerUp={handleAudioSegmentPointerUp}
            onPointerCancel={handleAudioSegmentPointerUp}
          >
            <div className="narration-track-segment-trim-start absolute inset-y-0 left-0 z-[2] w-2 cursor-ew-resize" />
            <div className="narration-track-segment-trim-end absolute inset-y-0 right-0 z-[2] w-2 cursor-ew-resize" />
            {shouldRenderWaveform ? (
              <AudioWaveformLayer
                sourcePath={seg.rawAudioPath}
                duration={visible}
                gainPoints={volumePoints}
                getVolumeAtTime={getTrackVolumeAtTime}
                colorVariable="--secondary-color"
                topPx={4}
                bottomPx={24}
                sourceInSec={seg.inPoint}
                sourceOutSec={seg.outPoint}
                playbackRate={rate}
                gainTimeOffsetSec={seg.startTime}
              />
            ) : (
              <div
                className="narration-track-segment-content absolute inset-0 z-[1] flex min-w-0 items-center gap-1.5 overflow-hidden px-1.5 text-[10px] text-[var(--on-surface)]"
                style={contentMaskStyle}
              >
                <span className="min-w-0 max-w-full truncate font-medium">{seg.name}</span>
                {showSpeedBadge && (
                  <span className="narration-track-segment-speed ml-auto rounded bg-[var(--secondary-color)]/30 px-1 text-[9px] font-semibold leading-3">
                    {rate.toFixed(2)}×
                  </span>
                )}
                {isEstimatedAlignment && (
                  <span
                    className="narration-track-segment-alignment-badge ml-auto rounded bg-amber-500/25 px-1 text-[9px] font-semibold leading-3 text-amber-200"
                    title="Estimated boundary"
                  >
                    est
                  </span>
                )}
              </div>
              )}
          </div>
        );
      })}

      {viewMode === "volume" && onUpdateVolumePoints && (
        <TrackVolumeCurve
          duration={duration}
          points={volumePoints ?? [{ time: 0, volume: 1 }, { time: Math.max(duration, 0.0001), volume: 1 }]}
          colorVar="--secondary-color"
          onChange={onUpdateVolumePoints}
          beginBatch={beginBatch}
          commitBatch={commitBatch}
          onCommit={onCommitSegments}
        />
      )}

      {rangeSelect && rangeWidth > 2 && activeDragMode === "ctrl-range" && (
        <div
          className={`narration-track-time-range-drawer ${rangePillClassName} z-[6]`}
          style={{ left: rangeLeft, width: rangeWidth }}
        />
      )}
      {rangeSelect && rangeWidth > 2 && activeDragMode !== "ctrl-range" && (
        <div
          className="narration-track-range-select timeline-range-select absolute pointer-events-none z-5"
          style={{ left: rangeLeft, width: rangeWidth }}
        />
      )}
      {isDraggingAudioSegment && (
        <div className="narration-track-drag-shield pointer-events-none absolute inset-0 z-[7]" />
      )}
    </div>
  );
};
