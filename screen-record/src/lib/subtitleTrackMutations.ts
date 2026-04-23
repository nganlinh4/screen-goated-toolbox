import { buildTextSplitPreview } from '@/lib/textSplitPreview';
import {
  mergeTextSegmentsInRange,
  overlapsRange,
  replaceSegmentsInRanges,
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

function sortSubtitleSegments(segments: readonly SubtitleSegment[]): SubtitleSegment[] {
  return [...segments].sort((left, right) => left.startTime - right.startTime);
}

function summarizeSubtitleTail(segments: readonly SubtitleSegment[]) {
  return sortSubtitleSegments(segments)
    .slice(-3)
    .map((segment) => ({
      id: segment.id,
      startTime: Number(segment.startTime.toFixed(3)),
      endTime: Number(segment.endTime.toFixed(3)),
      text: segment.text.slice(0, 32),
    }));
}

function formatSubtitleTail(segments: readonly SubtitleSegment[]) {
  const tail = summarizeSubtitleTail(segments);
  if (tail.length === 0) return 'none';
  return tail.map((segment) =>
    `${segment.id.slice(-8)}@${segment.startTime.toFixed(2)}-${segment.endTime.toFixed(2)}:${segment.text.replace(/\s+/g, ' ')}`,
  ).join(' || ');
}

const PARTIAL_TAIL_RETAIN_SEC = 2.0;

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
  return mapConcreteTracks(segment, (track) => ({
    ...track,
    segments: track.segments.map((subtitle) =>
      subtitle.id === subtitleId
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
  return mapConcreteTracks(segment, (track) => {
    const clonedInserted = cloneSubtitleSegments(inserted);
    const nextSegments = replacementRanges.length > 0
      ? replaceSegmentsInRanges(track.segments, replacementRanges, clonedInserted)
      : clonedInserted;
    return {
      ...track,
      segments: nextSegments,
    };
  });
}

export function mergePartialOriginalSubtitleSegments(
  segment: VideoSegment,
  inserted: readonly SubtitleSegment[],
  replacementRanges: ReadonlyArray<Pick<TrackSelectionRange, 'startTime' | 'endTime'>> = [],
): VideoSegment {
  return mapConcreteTracks(segment, (track) => {
    const clonedInserted = cloneSubtitleSegments(inserted);
    if (replacementRanges.length === 0) {
      console.log(
        `[SubtitleGen][Webview][merge-partial] full-replace before=${track.segments.length} inserted=${clonedInserted.length} beforeTail=${formatSubtitleTail(track.segments)} insertedTail=${formatSubtitleTail(clonedInserted)}`,
      );
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
    const overlappedExisting = track.segments.filter((existing) =>
      replacementRanges.some((range) => overlapsRange(existing, range)),
    );
    const retained = track.segments.filter((existing) =>
      !replacementRanges.some((range) => overlapsRange(existing, range))
      || (
        !insertedById.has(existing.id)
        && (
          clonedInserted.length === 0
          || existing.endTime >= retainTailFrom
        )
      ),
    );
    const nextSegments = sortSubtitleSegments(retained.concat([...insertedById.values()]));

    const firstRange = replacementRanges[0];
    const lastRange = replacementRanges[replacementRanges.length - 1];
    console.log(
      `[SubtitleGen][Webview][merge-partial] ranges=${replacementRanges.length} window=${firstRange?.startTime.toFixed(2) ?? 'na'}-${lastRange?.endTime.toFixed(2) ?? 'na'} before=${track.segments.length} overlap=${overlappedExisting.length} inserted=${clonedInserted.length} retained=${retained.length} next=${nextSegments.length} beforeTail=${formatSubtitleTail(track.segments)} insertedTail=${formatSubtitleTail(clonedInserted)} nextTail=${formatSubtitleTail(nextSegments)}`,
    );

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
