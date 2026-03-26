import React, { useState, useMemo } from 'react';
import { Keyboard, MousePointer2, Scissors } from 'lucide-react';
import { CursorVisibilitySegment, KeystrokeEvent, VideoSegment } from '@/types/video';
import { clampVisibilitySegmentsToDuration } from '@/lib/cursorHiding';
import { useSettings } from '@/hooks/useSettings';
import {
  filterKeystrokeEventsByMode,
  KEYSTROKE_VISIBILITY_MARGIN_AFTER,
  KEYSTROKE_VISIBILITY_MARGIN_BEFORE
} from '@/lib/keystrokeVisibility';
import {
  getHandlePriorityThresholdTime,
  isTimeNearRangeBoundary,
} from "./trackHoverUtils";
import { useTrackRangeSelect } from './useTrackRangeSelect';

interface KeystrokeTrackProps {
  segment: VideoSegment;
  duration: number;
  editingKeystrokeSegmentId: string | null;
  onKeystrokeClick: (id: string, splitTime: number) => void;
  onHandleDragStart: (id: string, type: 'start' | 'end' | 'body', offset?: number) => void;
  onAddKeystrokeSegment?: (atTime?: number) => void;
  onKeystrokeHover?: (id: string | null) => void;
  onDeleteKeystrokeSegments?: (ids: string[]) => void;
  onSelectionChange?: (ids: string[]) => void;
}

interface TimeRange {
  startTime: number;
  endTime: number;
}

type InputKind = 'keyboard' | 'mouse';

interface TypedTimeRange extends TimeRange {
  kind: InputKind;
}

function getSegmentsForMode(segment: VideoSegment): CursorVisibilitySegment[] {
  const mode = segment.keystrokeMode ?? 'off';
  if (mode === 'keyboard') return segment.keyboardVisibilitySegments ?? [];
  if (mode === 'keyboardMouse') return segment.keyboardMouseVisibilitySegments ?? [];
  return [];
}

function getRawEventRanges(segment: VideoSegment, duration: number): TypedTimeRange[] {
  const mode = segment.keystrokeMode ?? 'off';
  if (mode === 'off') return [];
  const events = [...filterKeystrokeEventsByMode(segment.keystrokeEvents ?? [], mode)]
    .sort((a, b) => a.startTime - b.startTime);
  if (!events.length) return [];

  const safeDuration = Math.max(duration, 0);
  const delaySecRaw = segment.keystrokeDelaySec;
  const delaySec = typeof delaySecRaw === 'number' && Number.isFinite(delaySecRaw)
    ? Math.max(-1, Math.min(1, delaySecRaw))
    : 0;
  const ranges: TypedTimeRange[] = [];
  const effectiveEnds = events.map((event) => Math.min(event.endTime, safeDuration));

  for (let i = 0; i < events.length; i++) {
    const event: KeystrokeEvent = events[i];
    const startTime = Math.max(0, Math.min(safeDuration, event.startTime + delaySec));
    const endTime = Math.max(0, Math.min(safeDuration, effectiveEnds[i] + delaySec));
    if (endTime - startTime > 0.001) {
      ranges.push({
        startTime,
        endTime,
        kind: event.type === 'keyboard' ? 'keyboard' : 'mouse',
      });
    }
  }
  return ranges;
}

function mergeRanges(ranges: TimeRange[]): TimeRange[] {
  if (!ranges.length) return [];
  const sorted = [...ranges].sort((a, b) => a.startTime - b.startTime);
  const merged: TimeRange[] = [{ ...sorted[0] }];
  for (let i = 1; i < sorted.length; i++) {
    const previous = merged[merged.length - 1];
    const current = sorted[i];
    if (current.startTime <= previous.endTime + 0.001) {
      previous.endTime = Math.max(previous.endTime, current.endTime);
    } else {
      merged.push({ ...current });
    }
  }
  return merged;
}

function getVisualActivityRanges(rawRanges: TypedTimeRange[], duration: number): TimeRange[] {
  const safeDuration = Math.max(duration, 0);
  return mergeRanges(rawRanges.map((range) => ({
    startTime: Math.max(0, range.startTime - KEYSTROKE_VISIBILITY_MARGIN_BEFORE),
    endTime: Math.min(safeDuration, range.endTime + KEYSTROKE_VISIBILITY_MARGIN_AFTER),
  })));
}

function intersectRangeWithRanges(
  rangeStart: number,
  rangeEnd: number,
  ranges: TimeRange[]
): TimeRange[] {
  const intersections: TimeRange[] = [];
  for (const range of ranges) {
    const startTime = Math.max(rangeStart, range.startTime);
    const endTime = Math.min(rangeEnd, range.endTime);
    if (endTime - startTime > 0.001) intersections.push({ startTime, endTime });
  }
  return intersections;
}

function getEmptyRangesInsideSegment(
  segmentStart: number,
  segmentEnd: number,
  indicatorRanges: TimeRange[]
): TimeRange[] {
  if (segmentEnd - segmentStart <= 0.001) return [];
  if (!indicatorRanges.length) {
    return [{ startTime: segmentStart, endTime: segmentEnd }];
  }

  const sorted = [...indicatorRanges].sort((a, b) => a.startTime - b.startTime);
  const empties: TimeRange[] = [];
  let cursor = segmentStart;
  for (const range of sorted) {
    if (range.startTime > cursor + 0.001) {
      empties.push({
        startTime: cursor,
        endTime: Math.min(range.startTime, segmentEnd),
      });
    }
    cursor = Math.max(cursor, range.endTime);
    if (cursor >= segmentEnd) break;
  }
  if (cursor < segmentEnd - 0.001) {
    empties.push({ startTime: cursor, endTime: segmentEnd });
  }
  return empties;
}

function getSegmentInputKinds(
  segmentStart: number,
  segmentEnd: number,
  rawRanges: TypedTimeRange[]
): { hasKeyboard: boolean; hasMouse: boolean } {
  let hasKeyboard = false;
  let hasMouse = false;
  for (const range of rawRanges) {
    const overlapStart = Math.max(segmentStart, range.startTime);
    const overlapEnd = Math.min(segmentEnd, range.endTime);
    if (overlapEnd - overlapStart <= 0.001) continue;
    if (range.kind === 'keyboard') hasKeyboard = true;
    else hasMouse = true;
    if (hasKeyboard && hasMouse) break;
  }
  return { hasKeyboard, hasMouse };
}

export const KeystrokeTrack: React.FC<KeystrokeTrackProps> = ({
  segment,
  duration,
  editingKeystrokeSegmentId,
  onKeystrokeClick,
  onHandleDragStart,
  onAddKeystrokeSegment,
  onKeystrokeHover,
  onDeleteKeystrokeSegments,
  onSelectionChange,
}) => {
  const { t } = useSettings();
  const [hoverState, setHoverState] = useState<
    | { type: 'split'; x: number; time: number; segId: string }
    | { type: 'add'; x: number }
    | null
  >(null);
  const mode = segment.keystrokeMode ?? 'off';
  const safeDuration = Math.max(duration, 0.001);
  const segments = clampVisibilitySegmentsToDuration(getSegmentsForMode(segment), safeDuration);

  const {
    selectedIds, rangeSelect, trackRef, isDraggingRange,
    onSegmentPointerDown,
    handleTrackPointerDown, handleTrackPointerMove, handleTrackPointerUp,
  } = useTrackRangeSelect(segments, duration, onSelectionChange, onDeleteKeystrokeSegments);
  const rawEventRanges = useMemo(
    () => getRawEventRanges(segment, safeDuration),
    [segment.keystrokeEvents, segment.keystrokeMode, segment.keystrokeDelaySec, safeDuration]
  );
  const visualActivityRanges = useMemo(
    () => getVisualActivityRanges(rawEventRanges, safeDuration),
    [rawEventRanges, safeDuration]
  );

  const handleMouseMove = (e: React.MouseEvent<HTMLDivElement>) => {
    if (mode === 'off' || isDraggingRange.current) {
      setHoverState(null);
      return;
    }
    const rect = e.currentTarget.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const time = (x / rect.width) * safeDuration;
    const thresholdTime = getHandlePriorityThresholdTime(safeDuration, rect.width);

    const containing = segments.find(
      (seg) => time >= seg.startTime && time <= seg.endTime
    );
    if (containing) {
      const canSplit = time > containing.startTime + 0.15 && time < containing.endTime - 0.15;
      setHoverState(canSplit ? { type: 'split', x, time, segId: containing.id } : null);
      return;
    }
    if (isTimeNearRangeBoundary(time, segments, thresholdTime)) {
      setHoverState(null);
      return;
    }
    setHoverState({ type: 'add', x });
  };

  return (
    <div
      className="keystroke-track timeline-lane relative h-7"
      title={t.trackKeystrokes}
      aria-label={t.trackKeystrokes}
      ref={trackRef}
      onMouseMove={handleMouseMove}
      onMouseLeave={() => { if (!isDraggingRange.current) setHoverState(null); }}
      onPointerDown={handleTrackPointerDown}
      onPointerMove={handleTrackPointerMove}
      onPointerUp={handleTrackPointerUp}
    >
      {mode !== 'off' && visualActivityRanges.map((range, idx) => (
        <div
          key={`keystroke-activity-${idx}`}
          className="keystroke-activity-range absolute inset-y-0 rounded-sm pointer-events-none"
          style={{
            left: `${(range.startTime / safeDuration) * 100}%`,
            width: `${((range.endTime - range.startTime) / safeDuration) * 100}%`,
            backgroundColor: "color-mix(in srgb, var(--timeline-success-color) 18%, transparent)",
          }}
        />
      ))}

      {segments.map((seg) => (
        (() => {
          const indicatorRanges = intersectRangeWithRanges(seg.startTime, seg.endTime, visualActivityRanges);
          const emptyRanges = getEmptyRangesInsideSegment(seg.startTime, seg.endTime, indicatorRanges);
          const hasIndicators = indicatorRanges.length > 0;
          const segmentDuration = Math.max(seg.endTime - seg.startTime, 0.001);
          const segmentKinds = getSegmentInputKinds(seg.startTime, seg.endTime, rawEventRanges);
          const segmentIcon = (() => {
            if (!hasIndicators) return 'none' as const;
            if (mode === 'keyboard') return 'keyboard' as const;
            if (segmentKinds.hasKeyboard && segmentKinds.hasMouse) return 'both' as const;
            if (segmentKinds.hasMouse) return 'mouse' as const;
            return 'keyboard' as const;
          })();

          return (
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
              onMouseEnter={() => onKeystrokeHover?.(seg.id)}
              onMouseLeave={() => onKeystrokeHover?.(null)}
              className="keystroke-segment timeline-block absolute h-full cursor-move group"
              data-tone={hasIndicators ? "success" : "neutral"}
              data-style={hasIndicators ? "filled" : "quiet"}
              data-active={editingKeystrokeSegmentId === seg.id ? "true" : "false"}
              data-selected={selectedIds.has(seg.id) ? "true" : undefined}
              style={{
                left: `${(seg.startTime / safeDuration) * 100}%`,
                width: `${((seg.endTime - seg.startTime) / safeDuration) * 100}%`,
                zIndex: 2,
              }}
            >
              {emptyRanges.map((range, idx) => (
                <div
                  key={`${seg.id}-empty-${idx}`}
                  className="keystroke-empty-window absolute inset-y-0 rounded-sm pointer-events-none"
                  style={{
                    left: `${((range.startTime - seg.startTime) / segmentDuration) * 100}%`,
                    width: `${((range.endTime - range.startTime) / segmentDuration) * 100}%`,
                    backgroundImage: 'repeating-linear-gradient(135deg, rgba(100,116,139,0.20) 0 5px, rgba(100,116,139,0.07) 5px 10px)',
                  }}
                />
              ))}
              {hasIndicators && indicatorRanges.map((range, idx) => (
                <div
                  key={`${seg.id}-indicator-${idx}`}
                  className="keystroke-indicator-window absolute inset-y-0 rounded-sm pointer-events-none"
                  style={{
                    left: `${((range.startTime - seg.startTime) / segmentDuration) * 100}%`,
                    width: `${((range.endTime - range.startTime) / segmentDuration) * 100}%`,
                    backgroundColor: "color-mix(in srgb, var(--timeline-success-color) 24%, transparent)",
                  }}
                />
              ))}
              <div className="keystroke-segment-content absolute inset-0 flex items-center justify-center overflow-hidden px-1 pointer-events-none">
                <div
                  className={`keystroke-segment-icon keystroke-segment-icon-${segmentIcon} flex items-center gap-[2px] ${
                    hasIndicators
                      ? 'text-[var(--timeline-success-color)]'
                      : 'text-[var(--timeline-neutral-color)]/80'
                  }`}
                >
                  {segmentIcon === 'none' && (
                    <span className="keystroke-segment-icon-empty text-[10px] leading-none">·</span>
                  )}
                  {(segmentIcon === 'keyboard' || segmentIcon === 'both') && (
                    <Keyboard className="keystroke-segment-icon-keyboard w-2.5 h-2.5" strokeWidth={2.2} />
                  )}
                  {(segmentIcon === 'mouse' || segmentIcon === 'both') && (
                    <MousePointer2 className="keystroke-segment-icon-mouse w-2.5 h-2.5" strokeWidth={2.2} />
                  )}
                </div>
              </div>
              <div
                className="keystroke-handle-start absolute inset-y-0 -left-[2px] w-[5px] cursor-ew-resize flex items-center justify-center opacity-0 group-hover:opacity-100 z-10"
                onPointerDown={(e) => { e.stopPropagation(); onHandleDragStart(seg.id, 'start'); }}
              >
                <div
                  className="keystroke-handle-bar timeline-handle-pill"
                />
              </div>
              <div
                className="keystroke-handle-end absolute inset-y-0 -right-[2px] w-[5px] cursor-ew-resize flex items-center justify-center opacity-0 group-hover:opacity-100 z-10"
                onPointerDown={(e) => { e.stopPropagation(); onHandleDragStart(seg.id, 'end'); }}
              >
                <div
                  className="keystroke-handle-bar timeline-handle-pill"
                />
              </div>
            </div>
          );
        })()
      ))}

      {rangeSelect && Math.abs(rangeSelect.endX - rangeSelect.startX) > 2 && (
        <div className="keystroke-range-select timeline-range-select absolute pointer-events-none z-5"
          style={{ left: Math.min(rangeSelect.startX, rangeSelect.endX), width: Math.abs(rangeSelect.endX - rangeSelect.startX) }} />
      )}

      {mode !== 'off' && hoverState && hoverState.type === 'split' && !isDraggingRange.current && (
        <button
          className="keystroke-split-btn timeline-arch-button absolute bottom-0 z-10 pointer-events-auto flex items-center justify-center"
          data-tone="accent"
          style={{ left: hoverState.x - 8 }}
          onPointerDown={(e) => {
            e.stopPropagation();
            onKeystrokeClick(hoverState.segId, hoverState.time);
            setHoverState(null);
          }}
        >
          <Scissors className="w-2 h-2" />
        </button>
      )}
      {mode !== 'off' && hoverState && hoverState.type === 'add' && onAddKeystrokeSegment && !isDraggingRange.current && (
        <button
          className="keystroke-add-btn timeline-arch-button absolute bottom-0 z-10 pointer-events-auto flex items-center justify-center text-[8px] font-bold"
          data-tone="success"
          style={{ left: hoverState.x - 8 }}
          onPointerDown={(e) => {
            e.stopPropagation();
            const rect = e.currentTarget.parentElement!.getBoundingClientRect();
            const time = (hoverState.x / rect.width) * safeDuration;
            onAddKeystrokeSegment(time);
            setHoverState(null);
          }}
        >
          +
        </button>
      )}
    </div>
  );
};
