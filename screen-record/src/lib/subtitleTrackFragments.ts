import type { TrackSelectionRange } from '@/lib/timelineSegmentSelection';
import type { SubtitleSegment } from '@/types/video';

export const PARTIAL_TAIL_RETAIN_SEC = 2.0;
const SUBTITLE_RANGE_EPSILON = 0.0001;

export function cloneSubtitleSegment(segment: SubtitleSegment): SubtitleSegment {
  return {
    ...segment,
    style: JSON.parse(JSON.stringify(segment.style)),
    sourceGroup: segment.sourceGroup
      ? JSON.parse(JSON.stringify(segment.sourceGroup))
      : undefined,
  };
}

export function cloneSubtitleSegments(segments: readonly SubtitleSegment[]): SubtitleSegment[] {
  return segments.map(cloneSubtitleSegment);
}

export function normalizeReplacementRanges(
  replacementRanges: ReadonlyArray<Pick<TrackSelectionRange, 'startTime' | 'endTime'>>,
) {
  return replacementRanges
    .map((range) => ({
      startTime: Math.min(range.startTime, range.endTime),
      endTime: Math.max(range.startTime, range.endTime),
    }))
    .filter((range) => range.endTime - range.startTime > SUBTITLE_RANGE_EPSILON)
    .sort((left, right) => left.startTime - right.startTime)
    .reduce<Array<{ startTime: number; endTime: number }>>((merged, range) => {
      const previous = merged[merged.length - 1];
      if (!previous || range.startTime > previous.endTime + SUBTITLE_RANGE_EPSILON) {
        merged.push(range);
        return merged;
      }
      previous.endTime = Math.max(previous.endTime, range.endTime);
      return merged;
    }, []);
}

function cloneSubtitleFragment(
  segment: SubtitleSegment,
  startTime: number,
  endTime: number,
  preserveId: boolean,
): SubtitleSegment | null {
  if (endTime - startTime <= SUBTITLE_RANGE_EPSILON) {
    return null;
  }
  return {
    ...cloneSubtitleSegment(segment),
    id: preserveId ? segment.id : crypto.randomUUID(),
    startTime,
    endTime,
  };
}

export function fragmentSubtitleSegmentByRanges(
  segment: SubtitleSegment,
  replacementRanges: ReadonlyArray<Pick<TrackSelectionRange, 'startTime' | 'endTime'>>,
) {
  const normalizedRanges = normalizeReplacementRanges(replacementRanges);
  if (normalizedRanges.length === 0) {
    return [{ segment: cloneSubtitleSegment(segment), insideRange: false }];
  }

  const fragments: Array<{ segment: SubtitleSegment; insideRange: boolean }> = [];
  let cursor = segment.startTime;
  let preserveId = true;

  for (const range of normalizedRanges) {
    if (range.endTime <= cursor + SUBTITLE_RANGE_EPSILON) continue;
    if (range.startTime >= segment.endTime - SUBTITLE_RANGE_EPSILON) break;

    const outsideEnd = Math.min(segment.endTime, range.startTime);
    if (outsideEnd > cursor + SUBTITLE_RANGE_EPSILON) {
      const outsideFragment = cloneSubtitleFragment(segment, cursor, outsideEnd, preserveId);
      if (outsideFragment) {
        fragments.push({ segment: outsideFragment, insideRange: false });
        preserveId = false;
      }
    }

    const insideStart = Math.max(cursor, range.startTime);
    const insideEnd = Math.min(segment.endTime, range.endTime);
    if (insideEnd > insideStart + SUBTITLE_RANGE_EPSILON) {
      const insideFragment = cloneSubtitleFragment(segment, insideStart, insideEnd, preserveId);
      if (insideFragment) {
        fragments.push({ segment: insideFragment, insideRange: true });
        preserveId = false;
      }
      cursor = insideEnd;
    }
  }

  if (cursor < segment.endTime - SUBTITLE_RANGE_EPSILON) {
    const trailingFragment = cloneSubtitleFragment(segment, cursor, segment.endTime, preserveId);
    if (trailingFragment) {
      fragments.push({ segment: trailingFragment, insideRange: false });
    }
  }

  return fragments;
}

export function preserveSubtitleSegmentsOutsideRanges(
  segments: readonly SubtitleSegment[],
  replacementRanges: ReadonlyArray<Pick<TrackSelectionRange, 'startTime' | 'endTime'>>,
) {
  return segments.flatMap((segment) =>
    fragmentSubtitleSegmentByRanges(segment, replacementRanges)
      .filter((fragment) => !fragment.insideRange)
      .map((fragment) => fragment.segment),
  );
}
