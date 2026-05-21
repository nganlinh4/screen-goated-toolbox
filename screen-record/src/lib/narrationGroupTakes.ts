import type { NarrationSegment } from "@/types/video";

function getVisibleSourceRange(segment: NarrationSegment) {
  const inPoint = Number.isFinite(segment.inPoint) ? Math.max(0, segment.inPoint) : 0;
  const outPoint = Number.isFinite(segment.outPoint)
    ? Math.max(inPoint, segment.outPoint)
    : Math.max(inPoint, segment.duration);
  return { inPoint, outPoint };
}

function makeGroupKey(segment: NarrationSegment) {
  if (segment.narrationGroupTakeId) return segment.narrationGroupTakeId;
  return null;
}

export function materializeNarrationGroupTakes(
  segments: readonly NarrationSegment[] | undefined,
): NarrationSegment[] {
  if (!segments?.length) return [];
  const singles: NarrationSegment[] = [];
  const groups = new Map<string, NarrationSegment[]>();

  for (const segment of segments) {
    const key = makeGroupKey(segment);
    if (!key) {
      singles.push(segment);
      continue;
    }
    const group = groups.get(key) ?? [];
    group.push(segment);
    groups.set(key, group);
  }

  const groupedSegments = [...groups.entries()].map(([key, group]) => {
    const sorted = [...group].sort((a, b) => a.startTime - b.startTime);
    const first = sorted[0];
    if (!first || sorted.length === 1) return first;
    const firstSource = getVisibleSourceRange(first);
    const lastSource = getVisibleSourceRange(sorted[sorted.length - 1]);
    const sourceIn = Math.min(...sorted.map((segment) => getVisibleSourceRange(segment).inPoint));
    const sourceOut = Math.max(...sorted.map((segment) => getVisibleSourceRange(segment).outPoint));
    const groupStart = Number.isFinite(first.narrationGroupSourceStartTime)
      ? first.narrationGroupSourceStartTime!
      : first.startTime - firstSource.inPoint;
    const sourceDuration = Math.max(0.05, sourceOut - sourceIn);
    const boundaryConfidence = Math.min(
      ...sorted.map((segment) => segment.narrationAlignmentConfidence ?? 0.4),
    );
    const sourceSubtitleIds = sorted.flatMap((segment) =>
      segment.sourceSubtitleIds?.length
        ? segment.sourceSubtitleIds
        : segment.sourceSubtitleId
          ? [segment.sourceSubtitleId]
          : [],
    );

    return {
      ...first,
      id: `group-take-${key}`,
      name: sorted.length > 1
        ? `${first.name || "Narration"} +${sorted.length - 1}`
        : first.name,
      startTime: groupStart + sourceIn,
      inPoint: sourceIn,
      outPoint: Math.max(sourceIn + 0.05, sourceOut, lastSource.outPoint),
      duration: Math.max(first.duration, sourceOut, sourceDuration),
      sourceSubtitleIds: [...new Set(sourceSubtitleIds)],
      narrationAlignmentConfidence: boundaryConfidence,
    } satisfies NarrationSegment;
  });

  return [...singles, ...groupedSegments].sort((a, b) => a.startTime - b.startTime);
}
