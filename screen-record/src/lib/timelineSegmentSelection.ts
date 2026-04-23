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

export function countSegmentsInRange<T extends { startTime: number; endTime: number }>(
  segments: readonly T[],
  range: Pick<TrackSelectionRange, "startTime" | "endTime"> | null | undefined,
) {
  if (!range) return 0;
  return segments.filter((segment) => overlapsRange(segment, range)).length;
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
    endTime: targets[targets.length - 1].endTime,
    text: mergedText,
  };
  const targetIds = new Set(targets.map((segment) => segment.id));
  const nextSegments = segments
    .filter((segment) => !targetIds.has(segment.id))
    .concat(merged)
    .sort((a, b) => a.startTime - b.startTime);

  return { merged, segments: nextSegments };
}

export function replaceSegmentsInRanges<T extends { startTime: number; endTime: number }>(
  segments: readonly T[],
  replacementRanges: ReadonlyArray<Pick<TrackSelectionRange, "startTime" | "endTime">>,
  inserted: readonly T[],
): T[] {
  if (replacementRanges.length === 0) {
    return [...inserted];
  }
  const retained = segments.filter(
    (segment) => !replacementRanges.some((range) => overlapsRange(segment, range)),
  );
  return retained.concat(inserted).sort((a, b) => a.startTime - b.startTime);
}
