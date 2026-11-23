import type { TextSegment } from "@/types/video";

export interface TrackSelectionRange {
  startTime: number;
  endTime: number;
  source: "selection" | "drag";
}

export function overlapsRange(
  segment: { startTime: number; endTime: number },
  range: Pick<TrackSelectionRange, "startTime" | "endTime">,
) {
  return segment.endTime > range.startTime && segment.startTime < range.endTime;
}

export function normalizeSelectionRange(
  range: Pick<TrackSelectionRange, "startTime" | "endTime"> | null | undefined,
  source: TrackSelectionRange["source"] = "selection",
): TrackSelectionRange | null {
  if (!range) return null;
  const startTime = Math.min(range.startTime, range.endTime);
  const endTime = Math.max(range.startTime, range.endTime);
  if (endTime - startTime <= 0.0001) return null;
  return { startTime, endTime, source };
}

export function deriveSelectionRangeFromIds<T extends { id: string; startTime: number; endTime: number }>(
  ids: readonly string[],
  segments: readonly T[],
): TrackSelectionRange | null {
  const idSet = new Set(ids);
  const matching = segments.filter((segment) => idSet.has(segment.id));
  if (matching.length === 0) return null;
  return {
    startTime: Math.min(...matching.map((segment) => segment.startTime)),
    endTime: Math.max(...matching.map((segment) => segment.endTime)),
    source: "selection",
  };
}

const DUPLICATE_GAP_SEC = 0.01;
const MIN_DUPLICATE_LENGTH_SEC = 0.05;

/**
 * Where a copy of `source` goes: right after it, shortened to end before the
 * next segment that occupies any time after the source (one starting exactly at
 * the source's end included). Null when no usable room is left before `limit`.
 */
export function findDuplicateSlot(
  segments: readonly { id: string; startTime: number; endTime: number }[],
  source: { id: string; startTime: number; endTime: number },
  limit: number,
): { startTime: number; endTime: number } | null {
  const length = source.endTime - source.startTime;
  if (length <= 0) return null;
  const startTime = source.endTime;
  const nextStart = Math.min(
    ...segments
      .filter((segment) => segment.id !== source.id && segment.endTime > startTime)
      .map((segment) => segment.startTime),
  );
  const maxEnd = Number.isFinite(nextStart) ? Math.min(nextStart - DUPLICATE_GAP_SEC, limit) : limit;
  const endTime = Math.min(startTime + length, maxEnd);
  return endTime - startTime < MIN_DUPLICATE_LENGTH_SEC ? null : { startTime, endTime };
}

export function mergeTextSegmentsInRange<T extends TextSegment>(
  segments: readonly T[],
  range: Pick<TrackSelectionRange, "startTime" | "endTime">,
  joiner: string,
): { merged: T | null; segments: T[] } {
  const targets = segments
    .filter((segment) => overlapsRange(segment, range))
    .sort((a, b) => a.startTime - b.startTime);
  if (targets.length < 2) {
    return { merged: null, segments: [...segments] };
  }

  const [first] = targets;
  const mergedText = targets
    .map((segment) => segment.text.trim())
    .filter(Boolean)
    .join(joiner)
    .trim();
  const merged: T = {
    ...first,
    startTime: targets[0].startTime,
    // The last segment by start can end before an earlier one that contains it.
    endTime: Math.max(...targets.map((segment) => segment.endTime)),
    text: mergedText,
  };
  const targetIds = new Set(targets.map((segment) => segment.id));
  const nextSegments = segments
    .filter((segment) => !targetIds.has(segment.id))
    .concat(merged)
    .sort((a, b) => a.startTime - b.startTime);

  return { merged, segments: nextSegments };
}
