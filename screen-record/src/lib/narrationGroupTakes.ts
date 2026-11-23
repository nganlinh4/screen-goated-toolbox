import type { NarrationSegment } from "@/types/video";
import { audioSegmentPlaybackRate } from "@/lib/audioSegmentTime";

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

function groupViewId(key: string) {
  return `group-take-${key}`;
}

function validGroupEdit(edit: NarrationSegment['narrationGroupEdit']) {
  return edit?.version === 1 && Number.isFinite(edit.startTime) && edit.startTime >= 0
    && Number.isFinite(edit.inPoint) && edit.inPoint >= 0
    && Number.isFinite(edit.outPoint) && edit.outPoint > edit.inPoint
    && Number.isFinite(edit.playbackRate) && edit.playbackRate > 0
    && (edit.outPointAutoExtend === undefined || typeof edit.outPointAutoExtend === 'boolean');
}

export function updateNarrationTake(
  segments: NarrationSegment[], id: string, patch: Partial<NarrationSegment>,
): NarrationSegment[] {
  const direct = segments.find((segment) => segment.id === id);
  if (direct && !direct.narrationGroupTakeId) {
    return segments.map((segment) => segment.id === id ? { ...segment, ...patch } : segment);
  }
  const view = materializeNarrationGroupTakes(segments).find((segment) => segment.id === id
    || (direct?.narrationGroupTakeId && segment.narrationGroupTakeId === direct.narrationGroupTakeId));
  if (!view?.narrationGroupTakeId) return segments;
  const previous = segments.find((segment) => segment.narrationGroupTakeId === view.narrationGroupTakeId
    && validGroupEdit(segment.narrationGroupEdit))?.narrationGroupEdit;
  const edit = {
    version: 1 as const,
    startTime: patch.startTime ?? view.startTime,
    inPoint: patch.inPoint ?? view.inPoint,
    outPoint: patch.outPoint ?? view.outPoint,
    playbackRate: audioSegmentPlaybackRate(patch.playbackRate ?? view.playbackRate),
    // Older saved edits have no marker and remain fixed. A new, untrimmed
    // edit follows later group members until the user explicitly trims it.
    outPointAutoExtend: patch.outPoint === undefined
      ? previous?.outPointAutoExtend ?? !previous
      : false,
  };
  if (!validGroupEdit(edit)) return segments;
  return segments.map((segment) => segment.narrationGroupTakeId === view.narrationGroupTakeId
    ? { ...segment, narrationGroupEdit: edit }
    : segment);
}

export function deleteNarrationTakes(segments: NarrationSegment[], ids: readonly string[]): NarrationSegment[] {
  const selected = new Set(ids);
  const groups = new Set(materializeNarrationGroupTakes(segments)
    .filter((segment) => selected.has(segment.id) && segment.narrationGroupTakeId)
    .map((segment) => segment.narrationGroupTakeId));
  // Callers can still hold a previously selected member ID, including IDs
  // saved before grouped views received a stable identity.
  for (const segment of segments) {
    if (selected.has(segment.id) && segment.narrationGroupTakeId) groups.add(segment.narrationGroupTakeId);
  }
  return segments.filter((segment) => !selected.has(segment.id) && !groups.has(segment.narrationGroupTakeId));
}

export function preserveNarrationGroupEdits(
  existing: readonly NarrationSegment[], incoming: readonly NarrationSegment[],
): NarrationSegment[] {
  const edits = new Map(existing.filter((segment) => segment.narrationGroupTakeId && validGroupEdit(segment.narrationGroupEdit))
    .map((segment) => [segment.narrationGroupTakeId, { path: segment.rawAudioPath, edit: segment.narrationGroupEdit }]));
  return incoming.map((segment) => {
    const previous = edits.get(segment.narrationGroupTakeId);
    return previous && previous.path === segment.rawAudioPath && !segment.narrationGroupEdit
      ? { ...segment, narrationGroupEdit: previous.edit }
      : segment;
  });
}

export function selectedNarrationSubtitleTimings(
  segments: readonly NarrationSegment[], selected: ReadonlySet<string>,
): Map<string, { startTime: number; endTime: number }> {
  const timings = new Map<string, { startTime: number; endTime: number }>();
  for (const view of materializeNarrationGroupTakes(segments)) {
    const members = view.id === groupViewId(view.narrationGroupTakeId ?? '')
      ? segments.filter((item) => item.narrationGroupTakeId === view.narrationGroupTakeId)
      : [view];
    if (!selected.has(view.id) && !members.some((member) => selected.has(member.id))) continue;
    const rate = audioSegmentPlaybackRate(view.playbackRate);
    for (const member of members) {
      if (!member.sourceSubtitleId) continue;
      const ids = member.sourceSubtitleIds ?? [member.sourceSubtitleId];
      // Legacy unsplit takes did not save individual cue boundaries. Inferring
      // them from edited cues would make repeated alignment destructive.
      const fractions = member.sourceSubtitleTimings ?? (ids.length > 1
        ? [] : [{ id: member.sourceSubtitleId, start: 0, end: 1 }]);
      for (const cue of fractions) {
        if (!ids.includes(cue.id) || !Number.isFinite(cue.start) || !Number.isFinite(cue.end)) continue;
        const length = member.outPoint - member.inPoint;
        const start = Math.max(view.inPoint, member.inPoint + Math.max(0, cue.start) * length);
        const end = Math.min(view.outPoint, member.inPoint + Math.min(1, cue.end) * length);
        if (end <= start) continue;
        timings.set(cue.id, {
          startTime: Math.max(0, view.startTime + (start - view.inPoint) / rate),
          endTime: Math.max(0, view.startTime + (end - view.inPoint) / rate),
        });
      }
    }
  }
  return timings;
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
    if (!first) return first;
    const edit = sorted.find((segment) => validGroupEdit(segment.narrationGroupEdit))?.narrationGroupEdit;
    if (sorted.length === 1) return edit ? {
      ...first, id: groupViewId(key), startTime: edit.startTime, inPoint: edit.inPoint,
      outPoint: edit.outPointAutoExtend ? Math.max(edit.outPoint, first.outPoint) : edit.outPoint,
      playbackRate: edit.playbackRate,
    } : { ...first, id: groupViewId(key) };
    const firstSource = getVisibleSourceRange(first);
    const rate = audioSegmentPlaybackRate(first.playbackRate);
    const lastSource = getVisibleSourceRange(sorted[sorted.length - 1]);
    const sourceIn = Math.min(...sorted.map((segment) => getVisibleSourceRange(segment).inPoint));
    const sourceOut = Math.max(...sorted.map((segment) => getVisibleSourceRange(segment).outPoint));
    const groupStart = Number.isFinite(first.narrationGroupSourceStartTime)
      ? first.narrationGroupSourceStartTime!
      : first.startTime - firstSource.inPoint / rate;
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
      id: groupViewId(key),
      name: sorted.length > 1
        ? `${first.name || "Narration"} +${sorted.length - 1}`
        : first.name,
      startTime: groupStart + sourceIn / rate,
      inPoint: sourceIn,
      outPoint: Math.max(sourceIn + 0.05, sourceOut, lastSource.outPoint),
      duration: Math.max(first.duration, sourceOut, sourceDuration),
      sourceSubtitleIds: [...new Set(sourceSubtitleIds)],
      narrationAlignmentConfidence: boundaryConfidence,
      ...(edit ? {
        startTime: edit.startTime,
        inPoint: edit.inPoint,
        outPoint: edit.outPointAutoExtend ? Math.max(edit.outPoint, sourceOut) : edit.outPoint,
        playbackRate: edit.playbackRate,
      } : {}),
    } satisfies NarrationSegment;
  });

  return [...singles, ...groupedSegments].sort((a, b) => a.startTime - b.startTime);
}
