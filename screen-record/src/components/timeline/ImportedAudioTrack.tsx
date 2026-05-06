import React, { useMemo } from "react";
import type { AudioGainPoint, ImportedAudioSegment } from "@/types/video";
import type { TrackSelectionRange } from "@/lib/timelineSegmentSelection";
import { useTrackRangeSelect } from "./useTrackRangeSelect";
import { TrackVolumeCurve } from "./TrackVolumeCurve";
import { useAudioSegmentTimelineEdit } from "./useAudioSegmentTimelineEdit";
import { AudioWaveformLayer } from "./AudioWaveformLayer";
import {
  SegmentBlocksCanvas,
  type TimelineVisibleRange,
  overlapsVisibleRange,
} from "./SegmentBlocksCanvas";

interface ImportedAudioTrackProps {
  segments: ImportedAudioSegment[];
  duration: number;
  onSegmentClick?: (id: string) => void;
  onDeleteSegments?: (ids: string[]) => void;
  selectedIds?: ReadonlySet<string>;
  selectedRange?: TrackSelectionRange | null;
  onSelectionChange?: (ids: string[]) => void;
  onRangeChange?: (range: TrackSelectionRange | null) => void;
  onUpdateSegment?: (id: string, patch: Partial<ImportedAudioSegment>) => void;
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

interface SelectableSegment {
  id: string;
  startTime: number;
  endTime: number;
}

export const ImportedAudioTrack: React.FC<ImportedAudioTrackProps> = ({
  segments,
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
  const safeDuration = Math.max(duration, 0.001);

  const selectable = useMemo<SelectableSegment[]>(
    () =>
      segments.map((seg) => {
        const trimmed = Math.max(seg.outPoint - seg.inPoint, MIN_VISIBLE_SEC);
        const rate = seg.playbackRate && seg.playbackRate > 0 ? seg.playbackRate : 1;
        const visible = Math.max(trimmed / rate, MIN_VISIBLE_SEC);
        return {
          id: seg.id,
          startTime: seg.startTime,
          endTime: seg.startTime + visible,
        };
      }),
    [segments],
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
  const denseMode = segments.length >= DENSE_SEGMENT_COUNT;
  const canvasSegments = useMemo(
    () =>
      selectable.map((segment) => ({
        ...segment,
        selected: effectiveSelectedIds.has(segment.id),
      })),
    [effectiveSelectedIds, selectable],
  );
  const {
    isDraggingAudioSegment,
    handleAudioSegmentPointerDown,
    handleAudioSegmentPointerMove,
    handleAudioSegmentPointerUp,
  } = useAudioSegmentTimelineEdit<ImportedAudioSegment>({
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
    "audio-track-range-pill pointer-events-none absolute inset-y-0 overflow-hidden rounded-md border border-[color:color-mix(in_srgb,var(--primary-color)_58%,transparent)] bg-[color:color-mix(in_srgb,var(--primary-color)_18%,transparent)]";
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
  const findSegmentAtTime = (time: number) =>
    segments.find((segment) => {
      const trimmed = Math.max(segment.outPoint - segment.inPoint, MIN_VISIBLE_SEC);
      const rate = segment.playbackRate && segment.playbackRate > 0 ? segment.playbackRate : 1;
      const visible = Math.max(trimmed / rate, MIN_VISIBLE_SEC);
      return time >= segment.startTime && time <= segment.startTime + visible;
    });

  const handleTrackPointerDownFast = (e: React.PointerEvent<HTMLDivElement>) => {
    if (e.target !== e.currentTarget || e.ctrlKey || !denseMode) {
      handleTrackPointerDown(e);
      return;
    }
    const rect = e.currentTarget.getBoundingClientRect();
    const time = ((e.clientX - rect.left) / Math.max(rect.width, 1)) * safeDuration;
    const hit = findSegmentAtTime(time);
    if (!hit) {
      handleTrackPointerDown(e);
      return;
    }
    e.stopPropagation();
    if (e.shiftKey) {
      addSegmentSelection(hit.id, { shiftKey: true });
      return;
    }
    addSegmentSelection(hit.id);
    onSegmentClick?.(hit.id);
    handleAudioSegmentPointerDown(e, hit);
  };

  return (
    <div
      ref={trackRef}
      className="audio-track timeline-lane relative h-7"
      onPointerDown={handleTrackPointerDownFast}
      onPointerMove={handleTrackPointerMove}
      onPointerUp={handleTrackPointerUp}
    >
      {denseMode && (
        <SegmentBlocksCanvas
          segments={canvasSegments}
          duration={duration}
          visibleRange={visibleTimeRange}
          colorVar="--primary-color"
          fallbackColor="#60a5fa"
          alpha={0.34}
        />
      )}

      {effectiveSelectedRange && (
        <div
          className={`audio-track-selected-range ${rangePillClassName} z-[2]`}
          style={{ left: selectedRangeLeftPct, width: selectedRangeWidthPct }}
        />
      )}

      {segments.map((seg) => {
        const trimmed = Math.max(seg.outPoint - seg.inPoint, MIN_VISIBLE_SEC);
        const rate = seg.playbackRate && seg.playbackRate > 0 ? seg.playbackRate : 1;
        const visible = Math.max(trimmed / rate, MIN_VISIBLE_SEC);
        const widthPct = Math.min(100, (visible / safeDuration) * 100);
        const leftPct = Math.min(100, Math.max(0, (seg.startTime / safeDuration) * 100));
        const isSelected = effectiveSelectedIds.has(seg.id);
        const widthPx = (visible / safeDuration) * Math.max(canvasWidthPx, 1);
        if (
          denseMode &&
          !isSelected &&
          (widthPx < MIN_INTERACTIVE_SEGMENT_PX ||
            !overlapsVisibleRange(seg.startTime, seg.startTime + visible, visibleTimeRange))
        ) {
          return null;
        }
        const showSpeedBadge = Math.abs(rate - 1) > 0.001;
        const shouldRenderWaveform =
          viewMode === "volume" &&
          widthPx >= MIN_WAVEFORM_SEGMENT_PX &&
          (!denseMode || isSelected || overlapsVisibleRange(seg.startTime, seg.startTime + visible, visibleTimeRange));
        return (
          <div
            key={seg.id}
            className="audio-track-segment timeline-block absolute h-full cursor-move group"
            data-tone="primary"
            data-selected={isSelected ? "true" : undefined}
            style={{
              left: `${leftPct}%`,
              width: `${widthPct}%`,
              background: "color-mix(in srgb, var(--primary-color) 22%, var(--ui-surface-3))",
              borderColor: isSelected
                ? "var(--primary-color)"
                : "color-mix(in srgb, var(--primary-color) 56%, var(--timeline-lane-border))",
              boxShadow: isSelected
                ? "0 0 0 1px var(--primary-color), 0 0 10px color-mix(in srgb, var(--primary-color) 28%, transparent)"
                : "0 0 0 1px color-mix(in srgb, var(--primary-color) 32%, transparent)",
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
              onSegmentClick?.(seg.id);
              handleAudioSegmentPointerDown(e, seg);
            }}
            onPointerMove={handleAudioSegmentPointerMove}
            onPointerUp={handleAudioSegmentPointerUp}
            onPointerCancel={handleAudioSegmentPointerUp}
          >
            <div className="audio-track-segment-trim-start absolute inset-y-0 left-0 z-[2] w-2 cursor-ew-resize" />
            <div className="audio-track-segment-trim-end absolute inset-y-0 right-0 z-[2] w-2 cursor-ew-resize" />
            {shouldRenderWaveform ? (
              <AudioWaveformLayer
                sourcePath={seg.rawAudioPath}
                duration={visible}
                gainPoints={volumePoints}
                getVolumeAtTime={getTrackVolumeAtTime}
                colorVariable="--primary-color"
                topPx={4}
                bottomPx={24}
                sourceInSec={seg.inPoint}
                sourceOutSec={seg.outPoint}
                playbackRate={rate}
                gainTimeOffsetSec={seg.startTime}
              />
            ) : (
              <div className="audio-track-segment-content absolute inset-0 z-[1] flex items-center gap-1.5 overflow-hidden px-1.5 text-[10px] text-[var(--on-surface)]">
                <span className="truncate font-medium">{seg.name}</span>
                {showSpeedBadge && (
                  <span className="audio-track-segment-speed ml-auto rounded bg-[var(--primary-color)]/30 px-1 text-[9px] font-semibold leading-3">
                    {rate.toFixed(2)}×
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
          colorVar="--primary-color"
          onChange={onUpdateVolumePoints}
          beginBatch={beginBatch}
          commitBatch={commitBatch}
          onCommit={onCommitSegments}
        />
      )}

      {rangeSelect && rangeWidth > 2 && activeDragMode === "ctrl-range" && (
        <div
          className={`audio-track-time-range-drawer ${rangePillClassName} z-[6]`}
          style={{ left: rangeLeft, width: rangeWidth }}
        />
      )}
      {rangeSelect && rangeWidth > 2 && activeDragMode !== "ctrl-range" && (
        <div
          className="audio-track-range-select timeline-range-select absolute pointer-events-none z-5"
          style={{ left: rangeLeft, width: rangeWidth }}
        />
      )}
      {isDraggingAudioSegment && (
        <div className="audio-track-drag-shield pointer-events-none absolute inset-0 z-[7]" />
      )}
    </div>
  );
};
