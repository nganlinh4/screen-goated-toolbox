import React, { useState } from "react";
import { Scissors } from '@/components/ui/MaterialIcon';
import {
  getHandlePriorityThresholdTime,
  isTimeNearRangeBoundary,
} from "./trackHoverUtils";
import {
  AUDIO_SEGMENT_DEFAULT_SEC,
  AUDIO_SEGMENT_MIN_SEC,
  AUDIO_SEGMENT_SPLIT_GAP_SEC,
  buildPointsFromRanges,
  clamp,
  getRangesFromPoints,
  resolveRanges,
  type SoftAudioPoint,
  type SoftAudioRange,
} from "./softAudioRanges";

import { startTimelinePointerDrag } from "./timelinePointerDrag";
import { timelineRangeKeyProps } from "./timelineKeyboard";
import { useSettings } from "@/hooks/useSettings";

export type { SoftAudioPoint } from "./softAudioRanges";

type RangeEdit = "start" | "end" | "body";

/** A range with its start, end or whole body moved by `delta` seconds, kept inside the lane. */
function moveSoftRange(range: SoftAudioRange, mode: RangeEdit, delta: number, duration: number): Partial<SoftAudioRange> {
  if (mode === "start") {
    return { startTime: clamp(range.startTime + delta, 0, range.endTime - AUDIO_SEGMENT_MIN_SEC), endTime: range.endTime };
  }
  if (mode === "end") {
    return { startTime: range.startTime, endTime: clamp(range.endTime + delta, range.startTime + AUDIO_SEGMENT_MIN_SEC, duration) };
  }
  const length = range.endTime - range.startTime;
  const startTime = clamp(range.startTime + delta, 0, duration - length);
  return { startTime, endTime: startTime + length, offset: startTime - range.startTime };
}

interface SoftAudioSegmentBlockProps<TPoint extends SoftAudioPoint> {
  classNamePrefix: string;
  /** Lane name used for each segment's accessible label. */
  laneName: string;
  duration: number;
  points: readonly TPoint[];
  isAvailable: boolean;
  tone: string;
  colorVariable: string;
  onUpdatePoints: (points: TPoint[]) => void;
  beginBatch: () => void;
  commitBatch: () => void;
  /** Playhead time for keyboard split (S) and add (A). */
  getPlayheadTime?: () => number;
}

export function SoftAudioSegmentBlock<TPoint extends SoftAudioPoint>({
  classNamePrefix,
  laneName,
  duration,
  points,
  isAvailable,
  tone,
  colorVariable,
  onUpdatePoints,
  beginBatch,
  commitBatch,
  getPlayheadTime,
}: SoftAudioSegmentBlockProps<TPoint>) {
  const { t } = useSettings();
  const [hoverState, setHoverState] = useState<
    | { type: "split"; x: number; time: number; range: SoftAudioRange }
    | { type: "add"; x: number; time: number }
    | null
  >(null);
  const safeDuration = Math.max(duration, AUDIO_SEGMENT_MIN_SEC);
  const ranges = getRangesFromPoints(points, safeDuration);

  // Every edit maps ranges of the current envelope to new ranges; each mapped
  // range carries its source's interior points so custom automation survives.
  const applyRanges = (nextRanges: readonly SoftAudioRange[]) => {
    onUpdatePoints(
      buildPointsFromRanges(
        safeDuration,
        resolveRanges(nextRanges, safeDuration),
        points,
        ranges,
      ) as TPoint[],
    );
  };
  const withSource = (range: SoftAudioRange): SoftAudioRange => ({ ...range, sourceId: range.id });

  const updateRange = (rangeId: string, patch: Partial<SoftAudioRange>) => {
    applyRanges(
      ranges.map((range) =>
        range.id === rangeId ? { ...withSource(range), ...patch } : withSource(range),
      ),
    );
  };

  const handleDragStart = (
    event: React.PointerEvent<HTMLDivElement>,
    range: SoftAudioRange,
    mode: RangeEdit,
  ) => {
    if (!isAvailable) return;
    event.stopPropagation();
    const track = event.currentTarget.closest<HTMLElement>("[data-soft-audio-track='true']");
    if (!track) return;
    const startRect = track.getBoundingClientRect();
    if (startRect.width <= 0) return;
    // Times come from the pointer position in the current rect, so follow-playhead
    // scrolling during the drag does not shift the block.
    const timeAt = (clientX: number, rect: DOMRect) =>
      ((clientX - rect.left) / rect.width) * safeDuration;
    const startPointerTime = timeAt(event.clientX, startRect);
    beginBatch();

    const onMove = (moveEvent: PointerEvent, rect: DOMRect) => {
      if (rect.width <= 0) return;
      const delta = timeAt(moveEvent.clientX, rect) - startPointerTime;
      updateRange(range.id, moveSoftRange(range, mode, delta, safeDuration));
    };

    startTimelinePointerDrag({ element: track, onMove, onEnd: () => commitBatch() });
  };

  const handleMouseMove = (event: React.MouseEvent<HTMLDivElement>) => {
    if (!isAvailable) return;
    const rect = event.currentTarget.getBoundingClientRect();
    if (rect.width <= 0) return;
    const x = event.clientX - rect.left;
    const time = (x / rect.width) * safeDuration;
    const thresholdTime = getHandlePriorityThresholdTime(safeDuration, rect.width);
    const containing = ranges.find(
      (range) => time >= range.startTime && time <= range.endTime,
    );
    if (containing) {
      const canSplit =
        time > containing.startTime + AUDIO_SEGMENT_MIN_SEC * 2 &&
        time < containing.endTime - AUDIO_SEGMENT_MIN_SEC * 2;
      setHoverState(canSplit ? { type: "split", x, time, range: containing } : null);
      return;
    }
    if (isTimeNearRangeBoundary(time, ranges, thresholdTime)) {
      setHoverState(null);
      return;
    }
    setHoverState({ type: "add", x, time });
  };

  const addRange = (time: number) => {
    const segmentDuration = Math.min(
      AUDIO_SEGMENT_DEFAULT_SEC,
      Math.max(AUDIO_SEGMENT_MIN_SEC, safeDuration * 0.12),
    );
    const startTime = clamp(time - segmentDuration / 2, 0, safeDuration - segmentDuration);
    const nextRange: SoftAudioRange = {
      id: `added-${Date.now()}`,
      startTime,
      endTime: startTime + segmentDuration,
      volume: ranges[0]?.volume ?? 1,
    };
    beginBatch();
    applyRanges([...ranges.map(withSource), nextRange]);
    commitBatch();
  };

  const splitRange = (range: SoftAudioRange, time: number) => {
    const halfGap = AUDIO_SEGMENT_SPLIT_GAP_SEC / 2;
    const leftEnd = clamp(time - halfGap, range.startTime + AUDIO_SEGMENT_MIN_SEC, range.endTime);
    const rightStart = clamp(time + halfGap, range.startTime, range.endTime - AUDIO_SEGMENT_MIN_SEC);
    if (
      leftEnd - range.startTime < AUDIO_SEGMENT_MIN_SEC ||
      range.endTime - rightStart < AUDIO_SEGMENT_MIN_SEC
    ) {
      return;
    }
    beginBatch();
    applyRanges(
      ranges.flatMap((item) =>
        item.id === range.id
          ? [
              { ...withSource(item), endTime: leftEnd },
              { ...withSource(item), id: `${item.id}-split`, startTime: rightStart },
            ]
          : [withSource(item)],
      ),
    );
    commitBatch();
  };

  return (
    <div
      className={`${classNamePrefix}-track timeline-lane relative h-7 ${
        isAvailable ? "" : "timeline-lane-unavailable"
      }`}
      data-soft-audio-track="true"
      onMouseMove={handleMouseMove}
      onMouseLeave={() => setHoverState(null)}
    >
      {ranges.map((range, rangeIndex) => {
        const rangeDuration = Math.max(AUDIO_SEGMENT_MIN_SEC, range.endTime - range.startTime);
        const keys = timelineRangeKeyProps({
          name: `${laneName} ${rangeIndex + 1}`,
          startLabel: t.timelineSegmentStart,
          endLabel: t.timelineSegmentEnd,
          range,
          duration: safeDuration,
          nudge: isAvailable
            ? (mode, delta) => {
                beginBatch();
                updateRange(range.id, moveSoftRange(range, mode, delta, safeDuration));
                commitBatch();
              }
            : undefined,
          onSplit: isAvailable ? (time) => splitRange(range, time) : undefined,
          onAdd: isAvailable ? addRange : undefined,
          getPlayheadTime,
        });
        return (
          <div
            key={range.id}
            {...keys.body}
            className={`${classNamePrefix}-soft-segment timeline-block absolute inset-y-0 overflow-hidden cursor-move group`}
            data-tone={tone}
            style={{
              left: `${(range.startTime / safeDuration) * 100}%`,
              width: `${(rangeDuration / safeDuration) * 100}%`,
            }}
            onPointerDown={(event) => handleDragStart(event, range, "body")}
          >
            <div
              className={`${classNamePrefix}-soft-segment-fill absolute inset-0`}
              style={{
                background: `color-mix(in srgb, var(${colorVariable}) 22%, var(--ui-surface-3))`,
              }}
            />
            <div
              className={`${classNamePrefix}-handle-start absolute inset-y-0 -left-[2px] z-10 flex w-[5px] cursor-ew-resize items-center justify-center opacity-0 group-hover:opacity-100 group-focus-within:opacity-100`}
              {...keys.start}
              onPointerDown={(event) => handleDragStart(event, range, "start")}
            >
              <div className={`${classNamePrefix}-handle-bar timeline-handle-pill`} />
            </div>
            <div
              className={`${classNamePrefix}-handle-end absolute inset-y-0 -right-[2px] z-10 flex w-[5px] cursor-ew-resize items-center justify-center opacity-0 group-hover:opacity-100 group-focus-within:opacity-100`}
              {...keys.end}
              onPointerDown={(event) => handleDragStart(event, range, "end")}
            >
              <div className={`${classNamePrefix}-handle-bar timeline-handle-pill`} />
            </div>
          </div>
        );
      })}

      {hoverState?.type === "split" && (
        <button
          className={`${classNamePrefix}-split-btn timeline-arch-button absolute bottom-0 z-10 flex items-center justify-center`}
          data-tone="accent"
          style={{ left: hoverState.x - 8 }}
          onPointerDown={(event) => {
            event.stopPropagation();
            splitRange(hoverState.range, hoverState.time);
            setHoverState(null);
          }}
        >
          <Scissors className="h-2 w-2" />
        </button>
      )}
      {hoverState?.type === "add" && (
        <button
          className={`${classNamePrefix}-add-btn timeline-arch-button absolute bottom-0 z-10 flex items-center justify-center text-[8px] font-bold`}
          data-tone={tone}
          style={{ left: hoverState.x - 8 }}
          onPointerDown={(event) => {
            event.stopPropagation();
            addRange(hoverState.time);
            setHoverState(null);
          }}
        >
          +
        </button>
      )}
    </div>
  );
}
