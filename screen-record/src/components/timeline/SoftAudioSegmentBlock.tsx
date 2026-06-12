import React, { useState } from "react";
import { Scissors } from '@/components/ui/MaterialIcon';
import {
  getHandlePriorityThresholdTime,
  isTimeNearRangeBoundary,
} from "./trackHoverUtils";

export interface SoftAudioPoint {
  time: number;
  volume: number;
}

interface SoftAudioRange {
  id: string;
  startTime: number;
  endTime: number;
  volume: number;
}

const AUDIO_SEGMENT_MIN_SEC = 0.05;
const AUDIO_SEGMENT_DEFAULT_SEC = 2;
const AUDIO_SEGMENT_EDGE_EPS_SEC = 0.001;
const AUDIO_SEGMENT_SPLIT_GAP_SEC = 0.04;
const AUDIO_SEGMENT_FADE_SEC = 0.12;
const AUDIBLE_VOLUME_EPS = 0.0001;

function clamp(value: number, min: number, max: number) {
  return Math.max(min, Math.min(max, value));
}

function normalizePoints(points: readonly SoftAudioPoint[], duration: number) {
  const safeDuration = Math.max(duration, AUDIO_SEGMENT_MIN_SEC);
  return [...points]
    .filter((point) => Number.isFinite(point.time) && Number.isFinite(point.volume))
    .map((point) => ({
      time: clamp(point.time, 0, safeDuration),
      volume: clamp(point.volume, 0, 1),
    }))
    .sort((a, b) => a.time - b.time);
}

function getRangesFromPoints(
  points: readonly SoftAudioPoint[],
  duration: number,
): SoftAudioRange[] {
  const safeDuration = Math.max(duration, AUDIO_SEGMENT_MIN_SEC);
  const sorted = normalizePoints(points, safeDuration);
  if (sorted.length === 0) return [];

  const ranges: SoftAudioRange[] = [];
  let startTime: number | null = null;
  let volume = 0;

  for (let index = 0; index < sorted.length; index += 1) {
    const point = sorted[index];
    const isAudible = point.volume > AUDIBLE_VOLUME_EPS;
    if (isAudible && startTime === null) {
      const previous = sorted[index - 1];
      startTime = previous && previous.volume <= AUDIBLE_VOLUME_EPS
        ? previous.time
        : point.time;
      volume = point.volume;
      continue;
    }

    if (isAudible) {
      volume = Math.max(volume, point.volume);
      continue;
    }

    if (startTime !== null) {
      const endTime = point.time;
      if (endTime - startTime >= AUDIO_SEGMENT_MIN_SEC) {
        ranges.push({
          id: `${startTime.toFixed(4)}-${endTime.toFixed(4)}-${ranges.length}`,
          startTime,
          endTime,
          volume: clamp(volume || 1, AUDIBLE_VOLUME_EPS, 1),
        });
      }
      startTime = null;
      volume = 0;
    }
  }

  if (startTime !== null && safeDuration - startTime >= AUDIO_SEGMENT_MIN_SEC) {
    ranges.push({
      id: `${startTime.toFixed(4)}-${safeDuration.toFixed(4)}-${ranges.length}`,
      startTime,
      endTime: safeDuration,
      volume: clamp(volume || 1, AUDIBLE_VOLUME_EPS, 1),
    });
  }

  return ranges;
}

function pushPoint(points: SoftAudioPoint[], duration: number, time: number, volume: number) {
  const clampedTime = clamp(time, 0, duration);
  const last = points[points.length - 1];
  if (last && Math.abs(last.time - clampedTime) < AUDIO_SEGMENT_EDGE_EPS_SEC / 2) {
    last.volume = volume;
    return;
  }
  points.push({ time: clampedTime, volume });
}

function buildPointsFromRanges(
  duration: number,
  ranges: readonly SoftAudioRange[],
): SoftAudioPoint[] {
  const safeDuration = Math.max(duration, AUDIO_SEGMENT_MIN_SEC);
  const normalizedRanges = [...ranges]
    .map((range) => ({
      ...range,
      startTime: clamp(range.startTime, 0, safeDuration - AUDIO_SEGMENT_MIN_SEC),
      endTime: clamp(range.endTime, AUDIO_SEGMENT_MIN_SEC, safeDuration),
      volume: clamp(range.volume, 0, 1),
    }))
    .filter((range) => range.endTime - range.startTime >= AUDIO_SEGMENT_MIN_SEC)
    .sort((a, b) => a.startTime - b.startTime);

  const points: SoftAudioPoint[] = [];
  pushPoint(points, safeDuration, 0, 0);
  for (const range of normalizedRanges) {
    const start = range.startTime;
    const end = range.endTime;
    const fade = Math.min(AUDIO_SEGMENT_FADE_SEC, Math.max(0, (end - start) / 2));
    if (start > AUDIO_SEGMENT_EDGE_EPS_SEC) {
      pushPoint(points, safeDuration, start - AUDIO_SEGMENT_EDGE_EPS_SEC, 0);
    }
    pushPoint(points, safeDuration, start, 0);
    pushPoint(points, safeDuration, start + fade, range.volume);
    pushPoint(points, safeDuration, end - fade, range.volume);
    pushPoint(points, safeDuration, end, 0);
    if (end < safeDuration - AUDIO_SEGMENT_EDGE_EPS_SEC) {
      pushPoint(points, safeDuration, end + AUDIO_SEGMENT_EDGE_EPS_SEC, 0);
    }
  }
  pushPoint(points, safeDuration, safeDuration, 0);
  return points.sort((a, b) => a.time - b.time);
}

function resolveRanges(
  ranges: readonly SoftAudioRange[],
  duration: number,
) {
  const safeDuration = Math.max(duration, AUDIO_SEGMENT_MIN_SEC);
  const sorted = [...ranges]
    .map((range) => ({
      ...range,
      startTime: clamp(range.startTime, 0, safeDuration - AUDIO_SEGMENT_MIN_SEC),
      endTime: clamp(range.endTime, AUDIO_SEGMENT_MIN_SEC, safeDuration),
    }))
    .filter((range) => range.endTime - range.startTime >= AUDIO_SEGMENT_MIN_SEC)
    .sort((a, b) => a.startTime - b.startTime);

  const resolved: SoftAudioRange[] = [];
  for (const range of sorted) {
    const last = resolved[resolved.length - 1];
    if (!last || range.startTime > last.endTime) {
      resolved.push(range);
      continue;
    }
    last.endTime = Math.max(last.endTime, range.endTime);
    last.volume = Math.max(last.volume, range.volume);
  }
  return resolved.filter((range) => range.endTime - range.startTime >= AUDIO_SEGMENT_MIN_SEC);
}

interface SoftAudioSegmentBlockProps<TPoint extends SoftAudioPoint> {
  classNamePrefix: string;
  duration: number;
  points: readonly TPoint[];
  isAvailable: boolean;
  tone: string;
  colorVariable: string;
  onUpdatePoints: (points: TPoint[]) => void;
  beginBatch: () => void;
  commitBatch: () => void;
}

export function SoftAudioSegmentBlock<TPoint extends SoftAudioPoint>({
  classNamePrefix,
  duration,
  points,
  isAvailable,
  tone,
  colorVariable,
  onUpdatePoints,
  beginBatch,
  commitBatch,
}: SoftAudioSegmentBlockProps<TPoint>) {
  const [hoverState, setHoverState] = useState<
    | { type: "split"; x: number; time: number; range: SoftAudioRange }
    | { type: "add"; x: number; time: number }
    | null
  >(null);
  const safeDuration = Math.max(duration, AUDIO_SEGMENT_MIN_SEC);
  const ranges = getRangesFromPoints(points, safeDuration);

  const applyRanges = (nextRanges: readonly SoftAudioRange[]) => {
    onUpdatePoints(buildPointsFromRanges(safeDuration, nextRanges) as TPoint[]);
  };

  const updateRange = (rangeId: string, patch: Partial<SoftAudioRange>) => {
    const nextRanges = resolveRanges(
      ranges.map((range) => (range.id === rangeId ? { ...range, ...patch } : range)),
      safeDuration,
    );
    applyRanges(nextRanges);
  };

  const handleDragStart = (
    event: React.PointerEvent<HTMLDivElement>,
    range: SoftAudioRange,
    mode: "start" | "end" | "body",
  ) => {
    if (!isAvailable) return;
    event.stopPropagation();
    const track = event.currentTarget.closest<HTMLElement>("[data-soft-audio-track='true']");
    if (!track) return;
    const rect = track.getBoundingClientRect();
    if (rect.width <= 0) return;
    const startClientX = event.clientX;
    const initialStart = range.startTime;
    const initialEnd = range.endTime;
    const initialDuration = initialEnd - initialStart;
    beginBatch();

    const onMove = (moveEvent: MouseEvent) => {
      const delta = ((moveEvent.clientX - startClientX) / rect.width) * safeDuration;
      if (mode === "start") {
        updateRange(range.id, {
          startTime: clamp(initialStart + delta, 0, initialEnd - AUDIO_SEGMENT_MIN_SEC),
          endTime: initialEnd,
        });
        return;
      }
      if (mode === "end") {
        updateRange(range.id, {
          startTime: initialStart,
          endTime: clamp(initialEnd + delta, initialStart + AUDIO_SEGMENT_MIN_SEC, safeDuration),
        });
        return;
      }
      const maxStart = safeDuration - initialDuration;
      const nextStart = clamp(initialStart + delta, 0, maxStart);
      updateRange(range.id, {
        startTime: nextStart,
        endTime: nextStart + initialDuration,
      });
    };

    const onUp = () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
      commitBatch();
    };

    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
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
    applyRanges(resolveRanges([...ranges, nextRange], safeDuration));
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
              { ...item, endTime: leftEnd },
              { ...item, id: `${item.id}-split`, startTime: rightStart },
            ]
          : [item],
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
      {ranges.map((range) => {
        const rangeDuration = Math.max(AUDIO_SEGMENT_MIN_SEC, range.endTime - range.startTime);
        const fadePct = `${Math.max(0, Math.min(50, (AUDIO_SEGMENT_FADE_SEC / rangeDuration) * 100))}%`;
        return (
          <div
            key={range.id}
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
              className={`${classNamePrefix}-soft-segment-fade-in absolute inset-y-0 left-0`}
              style={{
                width: fadePct,
                background: `linear-gradient(to right, transparent, color-mix(in srgb, var(${colorVariable}) 16%, transparent))`,
              }}
            />
            <div
              className={`${classNamePrefix}-soft-segment-fade-out absolute inset-y-0 right-0`}
              style={{
                width: fadePct,
                background: `linear-gradient(to left, transparent, color-mix(in srgb, var(${colorVariable}) 16%, transparent))`,
              }}
            />
            <div
              className={`${classNamePrefix}-handle-start absolute inset-y-0 -left-[2px] z-10 flex w-[5px] cursor-ew-resize items-center justify-center opacity-0 group-hover:opacity-100`}
              onPointerDown={(event) => handleDragStart(event, range, "start")}
            >
              <div className={`${classNamePrefix}-handle-bar timeline-handle-pill`} />
            </div>
            <div
              className={`${classNamePrefix}-handle-end absolute inset-y-0 -right-[2px] z-10 flex w-[5px] cursor-ew-resize items-center justify-center opacity-0 group-hover:opacity-100`}
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
