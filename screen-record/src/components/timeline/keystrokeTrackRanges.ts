import type { CursorVisibilitySegment, KeystrokeEvent, VideoSegment } from '@/types/video';
import {
  filterKeystrokeEventsByMode,
  KEYSTROKE_VISIBILITY_MARGIN_AFTER,
  KEYSTROKE_VISIBILITY_MARGIN_BEFORE
} from '@/lib/keystrokeVisibility';

/**
 * Pure range math for the keystroke lane: activity ranges from raw events and,
 * per visibility segment, the indicator/empty windows and input kinds. Lookups
 * binary-search sorted ranges, so each segment only visits ranges near it.
 */
export interface TimeRange {
  startTime: number;
  endTime: number;
}

type InputKind = 'keyboard' | 'mouse';

export interface TypedTimeRange extends TimeRange {
  kind: InputKind;
}

export function getSegmentsForMode(segment: VideoSegment): CursorVisibilitySegment[] {
  const mode = segment.keystrokeMode ?? 'off';
  if (mode === 'keyboard') return segment.keyboardVisibilitySegments ?? [];
  if (mode === 'keyboardMouse') return segment.keyboardMouseVisibilitySegments ?? [];
  return [];
}

export function getRawEventRanges(segment: VideoSegment, duration: number): TypedTimeRange[] {
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

export function getVisualActivityRanges(rawRanges: TypedTimeRange[], duration: number): TimeRange[] {
  const safeDuration = Math.max(duration, 0);
  return mergeRanges(rawRanges.map((range) => ({
    startTime: Math.max(0, range.startTime - KEYSTROKE_VISIBILITY_MARGIN_BEFORE),
    endTime: Math.min(safeDuration, range.endTime + KEYSTROKE_VISIBILITY_MARGIN_AFTER),
  })));
}

/** First index whose `endTime` exceeds `time`; `ranges` must be sorted and disjoint. */
function firstRangeEndingAfter(ranges: readonly TimeRange[], time: number): number {
  let low = 0;
  let high = ranges.length;
  while (low < high) {
    const mid = (low + high) >>> 1;
    if (ranges[mid].endTime > time) high = mid;
    else low = mid + 1;
  }
  return low;
}

/** Intersections with merged (sorted, disjoint) activity ranges. */
function intersectRangeWithRanges(
  rangeStart: number,
  rangeEnd: number,
  ranges: readonly TimeRange[]
): TimeRange[] {
  const intersections: TimeRange[] = [];
  for (let index = firstRangeEndingAfter(ranges, rangeStart); index < ranges.length; index += 1) {
    const range = ranges[index];
    if (range.startTime >= rangeEnd) break;
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

/** Raw event ranges sorted by start plus the longest range, bounding any overlap search. */
export interface RawEventIndex {
  ranges: TypedTimeRange[];
  maxLength: number;
}

export function buildRawEventIndex(rawRanges: TypedTimeRange[]): RawEventIndex {
  const ranges = [...rawRanges].sort((a, b) => a.startTime - b.startTime);
  const maxLength = ranges.reduce((max, range) => Math.max(max, range.endTime - range.startTime), 0);
  return { ranges, maxLength };
}

function getSegmentInputKinds(
  segmentStart: number,
  segmentEnd: number,
  { ranges, maxLength }: RawEventIndex,
): { hasKeyboard: boolean; hasMouse: boolean } {
  let hasKeyboard = false;
  let hasMouse = false;
  // A range overlapping the segment starts after segmentStart - maxLength.
  const windowStart = segmentStart - maxLength;
  let low = 0;
  let high = ranges.length;
  while (low < high) {
    const mid = (low + high) >>> 1;
    if (ranges[mid].startTime >= windowStart) high = mid;
    else low = mid + 1;
  }
  for (let index = low; index < ranges.length; index += 1) {
    const range = ranges[index];
    if (range.startTime >= segmentEnd) break;
    const overlapStart = Math.max(segmentStart, range.startTime);
    const overlapEnd = Math.min(segmentEnd, range.endTime);
    if (overlapEnd - overlapStart <= 0.001) continue;
    if (range.kind === 'keyboard') hasKeyboard = true;
    else hasMouse = true;
    if (hasKeyboard && hasMouse) break;
  }
  return { hasKeyboard, hasMouse };
}

export interface KeystrokeSegmentAnalysis {
  indicatorRanges: TimeRange[];
  emptyRanges: TimeRange[];
  kinds: { hasKeyboard: boolean; hasMouse: boolean };
}

export function analyzeKeystrokeSegments(
  segments: readonly CursorVisibilitySegment[],
  visualActivityRanges: readonly TimeRange[],
  rawEventIndex: RawEventIndex,
): KeystrokeSegmentAnalysis[] {
  return segments.map((seg) => {
    const indicatorRanges = intersectRangeWithRanges(seg.startTime, seg.endTime, visualActivityRanges);
    return {
      indicatorRanges,
      emptyRanges: getEmptyRangesInsideSegment(seg.startTime, seg.endTime, indicatorRanges),
      kinds: getSegmentInputKinds(seg.startTime, seg.endTime, rawEventIndex),
    };
  });
}
