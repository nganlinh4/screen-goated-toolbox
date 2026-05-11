import type { TimelineVisibleRange } from "./SegmentBlocksCanvas";

export interface TimelineRangeSegment {
  id: string;
  startTime: number;
  endTime: number;
}

export interface TimelineRenderWindowOptions<T extends TimelineRangeSegment> {
  segments: readonly T[];
  duration: number;
  canvasWidthPx: number;
  visibleRange?: TimelineVisibleRange | null;
  denseMode: boolean;
  selectedIds?: ReadonlySet<string>;
  activeIds?: ReadonlySet<string>;
  minInteractivePx: number;
  maxInteractiveSegments?: number;
}

export class TimelineSegmentIndex<T extends TimelineRangeSegment> {
  readonly segments: readonly T[];

  constructor(segments: readonly T[]) {
    let sorted = true;
    for (let index = 1; index < segments.length; index += 1) {
      if (segments[index - 1].startTime > segments[index].startTime) {
        sorted = false;
        break;
      }
    }
    this.segments = sorted
      ? segments
      : [...segments].sort((left, right) => left.startTime - right.startTime);
  }

  hitTest(time: number): T | null {
    if (!Number.isFinite(time)) return null;
    const segments = this.segments;
    let lo = 0;
    let hi = segments.length - 1;
    let candidate = -1;

    while (lo <= hi) {
      const mid = (lo + hi) >> 1;
      if (segments[mid].startTime <= time) {
        candidate = mid;
        lo = mid + 1;
      } else {
        hi = mid - 1;
      }
    }

    for (let index = candidate; index >= 0; index -= 1) {
      const segment = segments[index];
      if (time >= segment.startTime && time <= segment.endTime) return segment;
    }
    return null;
  }

  query(range: TimelineVisibleRange | null | undefined): T[] {
    if (!range) return [...this.segments];
    const start = Math.min(range.startTime, range.endTime);
    const end = Math.max(range.startTime, range.endTime);
    const segments = this.segments;

    const result: T[] = [];
    for (const segment of segments) {
      if (segment.startTime > end) break;
      if (segment.endTime >= start) result.push(segment);
    }
    return result;
  }
}

function segmentWidthPx(segment: TimelineRangeSegment, duration: number, canvasWidthPx: number) {
  const safeDuration = Math.max(duration, 0.001);
  return ((segment.endTime - segment.startTime) / safeDuration) * Math.max(canvasWidthPx, 1);
}

export function buildTimelineRenderWindow<T extends TimelineRangeSegment>({
  segments,
  duration,
  canvasWidthPx,
  visibleRange,
  denseMode,
  selectedIds,
  activeIds,
  minInteractivePx,
  maxInteractiveSegments = 180,
}: TimelineRenderWindowOptions<T>) {
  const index = new TimelineSegmentIndex(segments);
  const canvasSegments = index.query(visibleRange);
  if (!denseMode) {
    return { index, canvasSegments, domSegments: [...segments] };
  }

  const byId = new Map(segments.map((segment) => [segment.id, segment]));
  const domById = new Map<string, T>();
  if (visibleRange) {
    const interactiveCandidates = canvasSegments.filter((segment) =>
      segmentWidthPx(segment, duration, canvasWidthPx) >= minInteractivePx,
    );
    const cappedCandidates = interactiveCandidates.length <= maxInteractiveSegments
      ? interactiveCandidates
      : [];
    for (const segment of cappedCandidates) {
      domById.set(segment.id, segment);
    }
  }
  for (const id of selectedIds ?? []) {
    const segment = byId.get(id);
    if (segment) domById.set(id, segment);
  }
  for (const id of activeIds ?? []) {
    const segment = byId.get(id);
    if (segment) domById.set(id, segment);
  }

  return {
    index,
    canvasSegments,
    domSegments: [...domById.values()].sort((left, right) => left.startTime - right.startTime),
  };
}
