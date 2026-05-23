const LARGE_OVERLAP_SEC = 0.35;
const NESTED_OVERLAP_MIN_SEC = 0.1;
const NESTED_OVERLAP_RATIO = 0.5;
export const TIMELINE_TIMING_CONFLICT_COLOR = '#facc15';

interface TimelineTimingSegment {
  id: string;
  startTime: number;
  endTime: number;
}

function shouldFlagOverlap(
  left: Pick<TimelineTimingSegment, 'startTime' | 'endTime'>,
  right: Pick<TimelineTimingSegment, 'startTime' | 'endTime'>,
) {
  const overlap = Math.min(left.endTime, right.endTime) - Math.max(left.startTime, right.startTime);
  if (overlap <= 0) return false;
  if (overlap >= LARGE_OVERLAP_SEC) return true;

  const leftDuration = Math.max(0, left.endTime - left.startTime);
  const rightDuration = Math.max(0, right.endTime - right.startTime);
  const shorterDuration = Math.min(leftDuration, rightDuration);
  if (shorterDuration <= 0) return false;
  return overlap >= NESTED_OVERLAP_MIN_SEC && overlap / shorterDuration >= NESTED_OVERLAP_RATIO;
}

export function getTimelineTimingConflictIds(
  segments: readonly TimelineTimingSegment[],
): Set<string> {
  const sorted = [...segments].sort((left, right) =>
    left.startTime - right.startTime || left.endTime - right.endTime,
  );
  const conflictIds = new Set<string>();

  for (let index = 0; index < sorted.length; index += 1) {
    const current = sorted[index];
    for (let nextIndex = index + 1; nextIndex < sorted.length; nextIndex += 1) {
      const next = sorted[nextIndex];
      if (next.startTime >= current.endTime) break;
      if (!shouldFlagOverlap(current, next)) continue;
      conflictIds.add(current.id);
      conflictIds.add(next.id);
    }
  }

  return conflictIds;
}
