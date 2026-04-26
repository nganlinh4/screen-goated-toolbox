import { buildTextSplitPreview } from '@/lib/textSplitPreview';
import {
  mergeTextSegmentsInRange,
  overlapsRange,
  type TrackSelectionRange,
} from '@/lib/timelineSegmentSelection';
import type { SubtitleSegment, SubtitleTrack, VideoSegment } from '@/types/video';
import {
  findTranslationTrackByLanguage,
  ORIGINAL_SUBTITLE_TRACK_ID,
  createTranslationTrack,
  getActiveSubtitleTrack,
  getActiveSubtitleView,
  getSubtitleTracks,
  normalizeSubtitleTrackState,
} from '@/lib/subtitleTracks';

function cloneSubtitleSegment(segment: SubtitleSegment): SubtitleSegment {
  return {
    ...segment,
    style: JSON.parse(JSON.stringify(segment.style)),
  };
}

function cloneSubtitleSegments(segments: readonly SubtitleSegment[]): SubtitleSegment[] {
  return segments.map(cloneSubtitleSegment);
}

function mapConcreteTracks(
  segment: VideoSegment,
  updater: (track: SubtitleTrack) => SubtitleTrack,
): VideoSegment {
  const normalized = normalizeSubtitleTrackState(segment);
  return normalizeSubtitleTrackState({
    ...normalized,
    subtitleTracks: (normalized.subtitleTracks ?? []).map((track) =>
      updater({
        ...track,
        segments: cloneSubtitleSegments(track.segments ?? []),
      }),
    ),
  });
}

function mapOriginalTrack(
  segment: VideoSegment,
  updater: (track: SubtitleTrack) => SubtitleTrack,
): VideoSegment {
  const normalized = normalizeSubtitleTrackState(segment);
  const subtitleTracks = normalized.subtitleTracks ?? [];
  const activeView = normalized.activeSubtitleView;
  const nextTracks = subtitleTracks.map((track) =>
    track.id === ORIGINAL_SUBTITLE_TRACK_ID
      ? updater({
          ...track,
          segments: cloneSubtitleSegments(track.segments ?? []),
        })
      : track,
  );
  const originalTrack = nextTracks.find((track) => track.id === ORIGINAL_SUBTITLE_TRACK_ID) ?? null;
  return {
    ...normalized,
    subtitleTracks: nextTracks,
    subtitleSegments:
      activeView?.kind === 'track' && activeView.trackId === ORIGINAL_SUBTITLE_TRACK_ID
        ? cloneSubtitleSegments(originalTrack?.segments ?? [])
        : normalized.subtitleSegments,
  };
}

function sortSubtitleSegments(segments: readonly SubtitleSegment[]): SubtitleSegment[] {
  return [...segments].sort((left, right) => left.startTime - right.startTime);
}

const PARTIAL_TAIL_RETAIN_SEC = 2.0;
const SUBTITLE_RANGE_EPSILON = 0.0001;

function normalizeReplacementRanges(
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

function fragmentSubtitleSegmentByRanges(
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

function preserveSubtitleSegmentsOutsideRanges(
  segments: readonly SubtitleSegment[],
  replacementRanges: ReadonlyArray<Pick<TrackSelectionRange, 'startTime' | 'endTime'>>,
) {
  return segments.flatMap((segment) =>
    fragmentSubtitleSegmentByRanges(segment, replacementRanges)
      .filter((fragment) => !fragment.insideRange)
      .map((fragment) => fragment.segment),
  );
}

export function getEditableSubtitleTrack(segment: VideoSegment | null | undefined): SubtitleTrack | null {
  const activeView = getActiveSubtitleView(segment);
  if (activeView.kind !== 'track') return null;
  return getActiveSubtitleTrack(segment);
}

export function getEditableSubtitleSegments(segment: VideoSegment | null | undefined): SubtitleSegment[] {
  return getEditableSubtitleTrack(segment)?.segments ?? [];
}

export function isOriginalSubtitleTrackActive(segment: VideoSegment | null | undefined): boolean {
  return getEditableSubtitleTrack(segment)?.id === ORIGINAL_SUBTITLE_TRACK_ID;
}

export function updateSubtitleTextsOnActiveTrack(
  segment: VideoSegment,
  targetIds: ReadonlySet<string>,
  updater: (subtitle: SubtitleSegment) => SubtitleSegment,
): VideoSegment {
  const activeTrack = getEditableSubtitleTrack(segment);
  if (!activeTrack) return normalizeSubtitleTrackState(segment);
  return mapConcreteTracks(segment, (track) =>
    track.id === activeTrack.id
      ? {
          ...track,
          segments: track.segments.map((subtitle) =>
            targetIds.has(subtitle.id) ? updater(subtitle) : subtitle,
          ),
        }
      : track,
  );
}

export function updateSubtitleStylesAcrossTracks(
  segment: VideoSegment,
  targetIds: ReadonlySet<string>,
  updater: (subtitle: SubtitleSegment) => SubtitleSegment,
): VideoSegment {
  return mapConcreteTracks(segment, (track) => ({
    ...track,
    segments: track.segments.map((subtitle) =>
      targetIds.has(subtitle.id)
        ? {
            ...updater(subtitle),
            text: subtitle.text,
          }
        : subtitle,
    ),
  }));
}

export function updateSubtitleTimingAcrossTracks(
  segment: VideoSegment,
  subtitleId: string,
  updater: (subtitle: SubtitleSegment) => SubtitleSegment,
): VideoSegment {
  return updateSubtitleTimingsAcrossTracks(segment, [subtitleId], updater);
}

export function updateSubtitleTimingsAcrossTracks(
  segment: VideoSegment,
  subtitleIds: readonly string[],
  updater: (subtitle: SubtitleSegment) => SubtitleSegment,
): VideoSegment {
  const idSet = new Set(subtitleIds);
  return mapConcreteTracks(segment, (track) => ({
    ...track,
    segments: track.segments.map((subtitle) =>
      idSet.has(subtitle.id)
        ? {
            ...updater(subtitle),
            text: subtitle.text,
            style: JSON.parse(JSON.stringify(subtitle.style)),
          }
        : subtitle,
    ),
  }));
}

export function deleteSubtitleIdsAcrossTracks(
  segment: VideoSegment,
  ids: readonly string[],
): VideoSegment {
  const idSet = new Set(ids);
  return mapConcreteTracks(segment, (track) => ({
    ...track,
    segments: track.segments.filter((subtitle) => !idSet.has(subtitle.id)),
  }));
}

export function addSubtitleAcrossTracks(
  segment: VideoSegment,
  subtitle: SubtitleSegment,
): VideoSegment {
  const clonedSubtitle = cloneSubtitleSegment(subtitle);
  return mapConcreteTracks(segment, (track) => ({
    ...track,
    segments: sortSubtitleSegments([...track.segments, cloneSubtitleSegment(clonedSubtitle)]),
  }));
}

export function duplicateSubtitleAcrossTracks(
  segment: VideoSegment,
  subtitleId: string,
  duration: number,
): { segment: VideoSegment; newSubtitleId: string | null } {
  const newSubtitleId = crypto.randomUUID();
  let didDuplicate = false;
  const safeDuration = Math.max(duration, 0);
  const nextSegment = mapConcreteTracks(segment, (track) => {
    const source = track.segments.find((subtitle) => subtitle.id === subtitleId);
    if (!source) return track;
    const length = source.endTime - source.startTime;
    if (length <= 0) return track;
    const next = track.segments
      .filter((subtitle) => subtitle.startTime > source.endTime)
      .sort((a, b) => a.startTime - b.startTime)[0];
    const desiredStart = source.endTime;
    const maxEnd = next ? next.startTime - 0.01 : safeDuration > 0 ? safeDuration : desiredStart + length;
    const clampedEnd = Math.min(desiredStart + length, maxEnd);
    if (clampedEnd - desiredStart < 0.05) return track;
    didDuplicate = true;
    const duplicate: SubtitleSegment = {
      ...cloneSubtitleSegment(source),
      id: newSubtitleId,
      startTime: desiredStart,
      endTime: clampedEnd,
    };
    return {
      ...track,
      segments: sortSubtitleSegments([...track.segments, duplicate]),
    };
  });
  return {
    segment: didDuplicate ? nextSegment : normalizeSubtitleTrackState(segment),
    newSubtitleId: didDuplicate ? newSubtitleId : null,
  };
}

export function splitSubtitleAcrossTracks(
  segment: VideoSegment,
  subtitleId: string,
  splitTime: number,
): { segment: VideoSegment; newSubtitleId: string | null } {
  const newSubtitleId = crypto.randomUUID();
  let didSplit = false;
  const nextSegment = mapConcreteTracks(segment, (track) => {
    const target = track.segments.find((subtitle) => subtitle.id === subtitleId);
    if (!target || splitTime <= target.startTime + 0.1 || splitTime >= target.endTime - 0.1) {
      return track;
    }
    const preview = buildTextSplitPreview({
      text: target.text,
      startTime: target.startTime,
      endTime: target.endTime,
      splitTime,
    });
    if (!preview) return track;
    didSplit = true;
    const left: SubtitleSegment = {
      ...cloneSubtitleSegment(target),
      endTime: splitTime - 0.01,
      text: preview.leftText,
    };
    const right: SubtitleSegment = {
      ...cloneSubtitleSegment(target),
      id: newSubtitleId,
      startTime: splitTime + 0.01,
      text: preview.rightText,
    };
    return {
      ...track,
      segments: sortSubtitleSegments(
        track.segments
          .filter((subtitle) => subtitle.id !== subtitleId)
          .concat(left, right),
      ),
    };
  });
  return { segment: didSplit ? nextSegment : normalizeSubtitleTrackState(segment), newSubtitleId: didSplit ? newSubtitleId : null };
}

export function mergeSubtitleSelectionAcrossTracks(
  segment: VideoSegment,
  range: Pick<TrackSelectionRange, 'startTime' | 'endTime'>,
): { segment: VideoSegment; mergedId: string | null } {
  let mergedId: string | null = null;
  const nextSegment = mapConcreteTracks(segment, (track) => {
    const result = mergeTextSegmentsInRange(track.segments, range, ' ');
    if (result.merged) {
      mergedId = mergedId ?? result.merged.id;
    }
    return {
      ...track,
      segments: result.segments,
    };
  });
  return { segment: mergedId ? nextSegment : normalizeSubtitleTrackState(segment), mergedId };
}

export function replaceOriginalSubtitleSegments(
  segment: VideoSegment,
  inserted: readonly SubtitleSegment[],
  replacementRanges: ReadonlyArray<Pick<TrackSelectionRange, 'startTime' | 'endTime'>> = [],
): VideoSegment {
  return mapOriginalTrack(segment, (track) => {
    const clonedInserted = cloneSubtitleSegments(inserted);
    const nextSegments = replacementRanges.length > 0
      ? sortSubtitleSegments(
          preserveSubtitleSegmentsOutsideRanges(track.segments, replacementRanges).concat(clonedInserted),
        )
      : clonedInserted;
    return {
      ...track,
      segments: nextSegments,
    };
  });
}

export function clearDerivedSubtitleTracks(segment: VideoSegment): VideoSegment {
  const normalized = normalizeSubtitleTrackState(segment);
  return normalizeSubtitleTrackState({
    ...normalized,
    subtitleTracks: (normalized.subtitleTracks ?? []).filter(
      (track) => track.id === ORIGINAL_SUBTITLE_TRACK_ID,
    ),
    activeSubtitleView: {
      kind: 'track',
      trackId: ORIGINAL_SUBTITLE_TRACK_ID,
    },
    subtitleCustomChain: undefined,
  });
}

export function mergePartialOriginalSubtitleSegments(
  segment: VideoSegment,
  inserted: readonly SubtitleSegment[],
  replacementRanges: ReadonlyArray<Pick<TrackSelectionRange, 'startTime' | 'endTime'>> = [],
): VideoSegment {
  return mapOriginalTrack(segment, (track) => {
    const clonedInserted = cloneSubtitleSegments(inserted);
    if (replacementRanges.length === 0) {
      return {
        ...track,
        segments: clonedInserted,
      };
    }

    const insertedById = new Map(clonedInserted.map((subtitle) => [subtitle.id, subtitle]));
    const insertedCoverageEnd = clonedInserted.reduce(
      (maxEnd, subtitle) => Math.max(maxEnd, subtitle.endTime),
      Number.NEGATIVE_INFINITY,
    );
    const retainTailFrom = Number.isFinite(insertedCoverageEnd)
      ? insertedCoverageEnd - PARTIAL_TAIL_RETAIN_SEC
      : Number.NEGATIVE_INFINITY;
    const preservedOutside = preserveSubtitleSegmentsOutsideRanges(track.segments, replacementRanges);
    const retainedInsideTail = track.segments.flatMap((existing) => {
      if (!replacementRanges.some((range) => overlapsRange(existing, range))) {
        return [];
      }
      if (insertedById.has(existing.id)) {
        return [];
      }
      if (clonedInserted.length > 0 && existing.endTime < retainTailFrom) {
        return [];
      }
      return fragmentSubtitleSegmentByRanges(existing, replacementRanges)
        .filter((fragment) => fragment.insideRange)
        .map((fragment) => fragment.segment);
    });
    const retained = preservedOutside.concat(retainedInsideTail);
    const nextSegments = sortSubtitleSegments(retained.concat([...insertedById.values()]));

    return {
      ...track,
      segments: nextSegments,
    };
  });
}

export function ensureTranslatedTrack(
  segment: VideoSegment,
  targetLanguage: string,
  trackId?: string | null,
  slotLabel?: string | null,
): { segment: VideoSegment; track: SubtitleTrack } {
  const normalized = normalizeSubtitleTrackState(segment);
  const existing = trackId
    ? (normalized.subtitleTracks ?? []).find((track) => track.id === trackId) ?? null
    : findTranslationTrackByLanguage(normalized, targetLanguage);
  const nextTrack = existing ?? createTranslationTrack(
    normalized,
    targetLanguage,
    slotLabel,
  );
  const subtitleTracks = existing
    ? (normalized.subtitleTracks ?? []).map((track) =>
        track.id === nextTrack.id
          ? {
              ...track,
              targetLanguage,
            }
          : track,
      )
    : [...(normalized.subtitleTracks ?? []), nextTrack];
  return {
    segment: normalizeSubtitleTrackState({
      ...normalized,
      subtitleTracks,
    }),
    track: {
      ...nextTrack,
      targetLanguage,
    },
  };
}

export function patchSubtitleTrackTexts(
  segment: VideoSegment,
  trackId: string,
  patches: ReadonlyMap<string, string>,
): VideoSegment {
  return mapConcreteTracks(segment, (track) =>
    track.id === trackId
      ? {
          ...track,
          segments: track.segments.map((subtitle) => {
            const translatedText = patches.get(subtitle.id);
            return translatedText === undefined
              ? subtitle
              : {
                  ...subtitle,
                  text: translatedText,
                };
          }),
        }
      : track,
  );
}

export function collectSubtitleIdsForTranslation(
  segment: VideoSegment | null | undefined,
  selectedSubtitleIds: readonly string[] | undefined,
  editingSubtitleId: string | null | undefined,
): string[] {
  if (!segment) return [];
  if ((selectedSubtitleIds?.length ?? 0) > 0) {
    return [...selectedSubtitleIds!];
  }
  if (editingSubtitleId) {
    return [editingSubtitleId];
  }
  const originalTrack = getSubtitleTracks(segment).find((track) => track.id === ORIGINAL_SUBTITLE_TRACK_ID);
  return originalTrack?.segments.map((subtitle) => subtitle.id) ?? [];
}
