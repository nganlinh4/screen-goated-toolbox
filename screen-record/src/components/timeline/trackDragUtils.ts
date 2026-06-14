export type DragOriginalBounds = { startTime: number; endTime: number };

/** Default gap (seconds) carved out between the two pieces of a split segment. */
export const SPLIT_GAP = 0.3;
/** Default minimum size (seconds) each resulting piece must keep, else no split. */
export const MIN_PIECE_SEC = 0.15;

type SplittableSegment = { id: string; startTime: number; endTime: number };

/**
 * Clamps one edge of a segment while dragging a resize handle.
 *
 * - `'start'`: keeps the new start within `[0, seg.endTime - minSec]`.
 * - `'end'`: keeps the new end within `[seg.startTime + minSec, duration]`.
 *
 * Returns the clamped time; callers spread it back onto the segment.
 */
export function clampSegmentEdge(
  seg: { startTime: number; endTime: number },
  mode: 'start' | 'end',
  newTime: number,
  duration: number,
  minSec = 0.1,
): number {
  if (mode === 'start') {
    return Math.min(Math.max(0, newTime), seg.endTime - minSec);
  }
  return Math.max(Math.min(duration, newTime), seg.startTime + minSec);
}

/**
 * Splits the visibility segment with `id` at `splitTime`, carving a `gap`-wide
 * hole centered on the split. The left piece keeps the original id; the right
 * piece gets a fresh id. Returns the new (sorted) segment array, or `null` when
 * the segment is missing or either resulting piece would be smaller than
 * `minPiece`. Callers own any duration clamping and store write-back.
 */
export function splitVisibilitySegmentAtTime<T extends SplittableSegment>(
  segments: readonly T[],
  id: string,
  splitTime: number,
  { gap = SPLIT_GAP, minPiece = MIN_PIECE_SEC }: { gap?: number; minPiece?: number } = {},
): T[] | null {
  const seg = segments.find((candidate) => candidate.id === id);
  if (!seg) return null;

  const half = gap / 2;
  const leftEnd = splitTime - half;
  const rightStart = splitTime + half;

  if (leftEnd - seg.startTime < minPiece || seg.endTime - rightStart < minPiece) {
    return null;
  }

  const left = { id: seg.id, startTime: seg.startTime, endTime: leftEnd } as T;
  const right = { id: crypto.randomUUID(), startTime: rightStart, endTime: seg.endTime } as T;

  return segments
    .filter((candidate) => candidate.id !== id)
    .concat([left, right])
    .sort((a, b) => a.startTime - b.startTime);
}

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
