import React, { useState } from 'react';
import { Keyboard, MousePointer2 } from 'lucide-react';
import { CursorVisibilitySegment, KeystrokeEvent, VideoSegment } from '@/types/video';
import { clampVisibilitySegmentsToDuration } from '@/lib/cursorHiding';
import {
  filterKeystrokeEventsByMode,
  KEYSTROKE_VISIBILITY_MARGIN_AFTER,
  KEYSTROKE_VISIBILITY_MARGIN_BEFORE
} from '@/lib/keystrokeVisibility';

interface KeystrokeTrackProps {
  segment: VideoSegment;
  duration: number;
  editingKeystrokeSegmentId: string | null;
  onKeystrokeClick: (id: string, splitTime: number) => void;
  onHandleDragStart: (id: string, type: 'start' | 'end' | 'body', offset?: number) => void;
  onAddKeystrokeSegment?: (atTime?: number) => void;
  onKeystrokeHover?: (id: string | null) => void;
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
}) => {
  const [hoverX, setHoverX] = useState<number | null>(null);
  const mode = segment.keystrokeMode ?? 'off';
  const safeDuration = Math.max(duration, 0.001);
  const segments = clampVisibilitySegmentsToDuration(getSegmentsForMode(segment), safeDuration);
  const rawEventRanges = getRawEventRanges(segment, safeDuration);
  const visualActivityRanges = getVisualActivityRanges(rawEventRanges, safeDuration);

  const handleMouseMove = (e: React.MouseEvent<HTMLDivElement>) => {
    if (mode === 'off') {
      setHoverX(null);
      return;
    }
    const rect = e.currentTarget.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const time = (x / rect.width) * safeDuration;
    const isOverSegment = segments.some(
      (seg) => time >= seg.startTime && time <= seg.endTime
    );
    setHoverX(isOverSegment ? null : x);
  };

  return (
    <div
      className="keystroke-track relative h-7 rounded"
      style={{ backgroundColor: 'var(--timeline-track-bg)' }}
      onMouseMove={handleMouseMove}
      onMouseLeave={() => setHoverX(null)}
    >
      {mode !== 'off' && visualActivityRanges.map((range, idx) => (
        <div
          key={`keystroke-activity-${idx}`}
          className="keystroke-activity-range absolute inset-y-0 rounded-sm bg-emerald-400/16 pointer-events-none"
          style={{
            left: `${(range.startTime / safeDuration) * 100}%`,
            width: `${((range.endTime - range.startTime) / safeDuration) * 100}%`,
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
              onMouseDown={(e) => {
                e.stopPropagation();
                const rect = e.currentTarget.parentElement!.getBoundingClientRect();
                const clickX = e.clientX - rect.left;
                const clickTime = (clickX / rect.width) * safeDuration;
                onHandleDragStart(seg.id, 'body', clickTime - seg.startTime);
              }}
              onClick={(e) => {
                e.stopPropagation();
                const rect = e.currentTarget.getBoundingClientRect();
                const frac = (e.clientX - rect.left) / rect.width;
                const time = seg.startTime + frac * (seg.endTime - seg.startTime);
                onKeystrokeClick(seg.id, time);
              }}
              onMouseEnter={() => onKeystrokeHover?.(seg.id)}
              onMouseLeave={() => onKeystrokeHover?.(null)}
              className={`keystroke-segment absolute h-full rounded cursor-move group border ${
                editingKeystrokeSegmentId === seg.id
                  ? 'ring-1 ring-emerald-400/70'
                  : ''
              } ${
                hasIndicators
                  ? 'border-emerald-300/50 bg-emerald-500/9 hover:bg-emerald-500/14'
                  : 'border-slate-300/34 border-dashed bg-slate-500/16 hover:bg-slate-500/24'
              }`}
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
                    backgroundImage: 'repeating-linear-gradient(135deg, rgba(148,163,184,0.20) 0 5px, rgba(148,163,184,0.06) 5px 10px)',
                  }}
                />
              ))}
              {hasIndicators && indicatorRanges.map((range, idx) => (
                <div
                  key={`${seg.id}-indicator-${idx}`}
                  className="keystroke-indicator-window absolute inset-y-0 rounded-sm bg-emerald-400/38 pointer-events-none"
                  style={{
                    left: `${((range.startTime - seg.startTime) / segmentDuration) * 100}%`,
                    width: `${((range.endTime - range.startTime) / segmentDuration) * 100}%`,
                  }}
                />
              ))}
              <div className="keystroke-segment-content absolute inset-0 flex items-center justify-center overflow-hidden px-1 pointer-events-none">
                <div
                  className={`keystroke-segment-icon keystroke-segment-icon-${segmentIcon} flex items-center gap-[2px] ${
                    hasIndicators ? 'text-emerald-200/85' : 'text-slate-200/55'
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
                onMouseDown={(e) => { e.stopPropagation(); onHandleDragStart(seg.id, 'start'); }}
              >
                <div
                  className="keystroke-handle-bar w-[3px] h-3 rounded-full shadow-[0_0_4px_rgba(0,0,0,0.4)]"
                  style={{ backgroundColor: 'var(--timeline-handle)' }}
                />
              </div>
              <div
                className="keystroke-handle-end absolute inset-y-0 -right-[2px] w-[5px] cursor-ew-resize flex items-center justify-center opacity-0 group-hover:opacity-100 z-10"
                onMouseDown={(e) => { e.stopPropagation(); onHandleDragStart(seg.id, 'end'); }}
              >
                <div
                  className="keystroke-handle-bar w-[3px] h-3 rounded-full shadow-[0_0_4px_rgba(0,0,0,0.4)]"
                  style={{ backgroundColor: 'var(--timeline-handle)' }}
                />
              </div>
            </div>
          );
        })()
      ))}

      {mode !== 'off' && hoverX !== null && onAddKeystrokeSegment && (
        <button
          className="keystroke-add-btn absolute top-1/2 -translate-y-1/2 w-4 h-4 rounded-full bg-emerald-500/50 hover:bg-emerald-500 flex items-center justify-center text-white text-[10px] leading-none font-bold transition-colors z-10 pointer-events-auto"
          style={{ left: hoverX - 8 }}
          onMouseDown={(e) => {
            e.stopPropagation();
            const rect = e.currentTarget.parentElement!.getBoundingClientRect();
            const time = (hoverX / rect.width) * safeDuration;
            onAddKeystrokeSegment(time);
            setHoverX(null);
          }}
        >
          +
        </button>
      )}
    </div>
  );
};
