export type DragOriginalBounds = { startTime: number; endTime: number };

export function snapshotSegmentBounds<T extends { id: string; startTime: number; endTime: number }>(
  segments: readonly T[],
  targetIds: readonly string[],
): Map<string, DragOriginalBounds> {
  const targetSet = new Set(targetIds);
  return new Map(
    segments
      .filter((segment) => targetSet.has(segment.id))
      .map((segment) => [
        segment.id,
        { startTime: segment.startTime, endTime: segment.endTime },
      ]),
  );
}

export function computeGroupDragDelta(
  originals: Map<string, DragOriginalBounds>,
  anchorId: string,
  dragOffset: number,
  newTime: number,
  duration: number,
): number | null {
  const anchor = originals.get(anchorId);
  if (!anchor) return null;
  let delta = (newTime - dragOffset) - anchor.startTime;
  let minStart = Number.POSITIVE_INFINITY;
  let maxEnd = Number.NEGATIVE_INFINITY;
  originals.forEach((value) => {
    minStart = Math.min(minStart, value.startTime);
    maxEnd = Math.max(maxEnd, value.endTime);
  });
  if (!Number.isFinite(minStart) || !Number.isFinite(maxEnd)) return null;
  if (minStart + delta < 0) delta = -minStart;
  if (maxEnd + delta > duration) delta = duration - maxEnd;
  return delta;
}
