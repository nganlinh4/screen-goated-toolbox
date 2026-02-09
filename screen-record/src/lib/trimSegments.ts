import type { TrimSegment, VideoSegment } from '@/types/video';

const MIN_SEGMENT_DURATION = 0.1;
const EPSILON = 0.0001;

function clamp(v: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, v));
}

export function mergeTrimSegments(segments: TrimSegment[]): TrimSegment[] {
  if (segments.length <= 1) return segments.map(s => ({ ...s }));
  const sorted = [...segments].sort((a, b) => a.startTime - b.startTime);
  const merged: TrimSegment[] = [{ ...sorted[0] }];
  for (let i = 1; i < sorted.length; i++) {
    const cur = sorted[i];
    const last = merged[merged.length - 1];
    if (cur.startTime <= last.endTime + EPSILON) {
      last.endTime = Math.max(last.endTime, cur.endTime);
    } else {
      merged.push({ ...cur });
    }
  }
  return merged;
}

export function getTrimSegments(segment: VideoSegment, duration: number): TrimSegment[] {
  const base =
    segment.trimSegments && segment.trimSegments.length > 0
      ? segment.trimSegments
      : [{
          id: crypto.randomUUID(),
          startTime: segment.trimStart,
          endTime: segment.trimEnd,
        }];

  const clipped = base
    .map(s => ({
      id: s.id || crypto.randomUUID(),
      startTime: clamp(s.startTime, 0, duration),
      endTime: clamp(s.endTime, 0, duration),
    }))
    .filter(s => s.endTime - s.startTime >= MIN_SEGMENT_DURATION);

  if (clipped.length === 0) {
    return [{
      id: crypto.randomUUID(),
      startTime: clamp(segment.trimStart, 0, duration),
      endTime: clamp(segment.trimEnd || duration, 0, duration),
    }];
  }

  return mergeTrimSegments(clipped);
}

export function getTrimBounds(
  segment: VideoSegment,
  duration: number
): { trimStart: number; trimEnd: number } {
  const segs = getTrimSegments(segment, duration);
  return {
    trimStart: segs[0].startTime,
    trimEnd: segs[segs.length - 1].endTime,
  };
}

export function getTotalTrimDuration(segment: VideoSegment, duration: number): number {
  return getTrimSegments(segment, duration).reduce((sum, s) => sum + (s.endTime - s.startTime), 0);
}

export function toCompactTime(sourceTime: number, segment: VideoSegment, duration: number): number {
  const segs = getTrimSegments(segment, duration);
  let compact = 0;
  for (const seg of segs) {
    if (sourceTime <= seg.startTime) return compact;
    if (sourceTime < seg.endTime) return compact + (sourceTime - seg.startTime);
    compact += seg.endTime - seg.startTime;
  }
  return compact;
}

export function toSourceTime(compactTime: number, segment: VideoSegment, duration: number): number {
  const segs = getTrimSegments(segment, duration);
  const total = segs.reduce((sum, s) => sum + (s.endTime - s.startTime), 0);
  let remaining = clamp(compactTime, 0, total);
  for (const seg of segs) {
    const len = seg.endTime - seg.startTime;
    if (remaining <= len) return seg.startTime + remaining;
    remaining -= len;
  }
  return segs[segs.length - 1].endTime;
}

export function clampToTrimSegments(sourceTime: number, segment: VideoSegment, duration: number): number {
  const segs = getTrimSegments(segment, duration);
  if (segs.length === 0) return clamp(sourceTime, 0, duration);

  if (sourceTime <= segs[0].startTime) return segs[0].startTime;
  if (sourceTime >= segs[segs.length - 1].endTime) return segs[segs.length - 1].endTime;

  for (let i = 0; i < segs.length; i++) {
    const seg = segs[i];
    if (sourceTime >= seg.startTime && sourceTime <= seg.endTime) return sourceTime;
    const next = segs[i + 1];
    if (next && sourceTime > seg.endTime && sourceTime < next.startTime) {
      const dPrev = sourceTime - seg.endTime;
      const dNext = next.startTime - sourceTime;
      return dNext < dPrev ? next.startTime : seg.endTime;
    }
  }
  return sourceTime;
}

export function getNextPlayableTime(
  sourceTime: number,
  segment: VideoSegment,
  duration: number
): number | null {
  const segs = getTrimSegments(segment, duration);
  for (const seg of segs) {
    if (sourceTime < seg.startTime - EPSILON) return seg.startTime;
    if (sourceTime >= seg.startTime - EPSILON && sourceTime < seg.endTime - EPSILON) return sourceTime;
  }
  return null;
}

export function normalizeSegmentTrimData(segment: VideoSegment, duration: number): VideoSegment {
  const segs = getTrimSegments(segment, duration);
  return {
    ...segment,
    trimSegments: segs,
    trimStart: segs[0].startTime,
    trimEnd: segs[segs.length - 1].endTime,
  };
}

export function sourceRangeToCompactRanges(
  start: number,
  end: number,
  segment: VideoSegment,
  duration: number
): Array<{ start: number; end: number }> {
  const segs = getTrimSegments(segment, duration);
  const result: Array<{ start: number; end: number }> = [];
  let compactCursor = 0;

  for (const seg of segs) {
    const overlapStart = Math.max(start, seg.startTime);
    const overlapEnd = Math.min(end, seg.endTime);
    if (overlapEnd - overlapStart > EPSILON) {
      result.push({
        start: compactCursor + (overlapStart - seg.startTime),
        end: compactCursor + (overlapEnd - seg.startTime),
      });
    }
    compactCursor += seg.endTime - seg.startTime;
  }
  return result;
}

