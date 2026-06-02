import {
  ORIGINAL_SUBTITLE_TRACK_ID,
  getTranslationSubtitleTrackId,
  normalizeSubtitleTrackState,
  setActiveSubtitleTrackView,
  upsertSubtitleTrack,
} from "@/lib/subtitleTracks";
import type {
  SubtitleChainItem,
  SubtitleSegment,
  SubtitleTrack,
  SubtitleViewState,
  VideoSegment,
} from "@/types/video";

export interface S2sSubtitleStateSnapshot {
  subtitleTracks?: SubtitleTrack[];
  activeSubtitleView?: SubtitleViewState;
  subtitleCustomChain?: SubtitleChainItem[];
  subtitleSegments?: SubtitleSegment[];
}

export interface PopulateS2sSubtitleTracksOptions {
  preserveExistingOutside?: boolean;
  baseSourceSegments?: SubtitleSegment[];
  baseTargetSegments?: SubtitleSegment[];
  replacementRanges?: Array<{ startTime: number; endTime: number }>;
  restoreSnapshot?: S2sSubtitleStateSnapshot | null;
  debugPhase?: string;
  liveUpdate?: boolean;
}

export function mergeS2sSubtitleSegments(
  existingSegments: readonly SubtitleSegment[],
  incomingSegments: readonly SubtitleSegment[],
  replacementRanges?: readonly { startTime: number; endTime: number }[],
) {
  const incomingIds = new Set(incomingSegments.map((segment) => segment.id));
  const normalizedRanges = replacementRanges
    ?.map((range) => ({
      startTime: Math.min(range.startTime, range.endTime),
      endTime: Math.max(range.startTime, range.endTime),
    }))
    .filter((range) => range.endTime - range.startTime > 0.001);
  if (incomingSegments.length === 0 && (!normalizedRanges || normalizedRanges.length === 0)) {
    return [...existingSegments];
  }
  const fallbackRanges =
    incomingSegments.length > 0
      ? incomingSegments.map((segment) => ({
          startTime: segment.startTime,
          endTime: segment.endTime,
        }))
      : [];
  const ranges =
    normalizedRanges && normalizedRanges.length > 0
      ? normalizedRanges
      : fallbackRanges;
  const epsilon = 0.001;
  const kept = existingSegments.filter((segment) => {
    if (incomingIds.has(segment.id)) return false;
    return !ranges.some(
      (range) =>
        segment.startTime < range.endTime - epsilon &&
        range.startTime + epsilon < segment.endTime,
    );
  });
  return [...kept, ...incomingSegments].sort((left, right) =>
    left.startTime === right.startTime
      ? left.endTime - right.endTime
      : left.startTime - right.startTime,
  );
}

export function replaceS2sSubtitleSegments(
  incomingSegments: readonly SubtitleSegment[],
) {
  const byId = new Map<string, SubtitleSegment>();
  for (const segment of incomingSegments) {
    byId.set(segment.id, segment);
  }
  return [...byId.values()].sort((left, right) =>
    left.startTime === right.startTime
      ? left.endTime - right.endTime
      : left.startTime - right.startTime,
  );
}

function cloneSubtitleSegment(segment: SubtitleSegment): SubtitleSegment {
  return {
    ...segment,
    style: segment.style ? { ...segment.style } : segment.style,
    sourceGroup: segment.sourceGroup ? { ...segment.sourceGroup } : segment.sourceGroup,
    provenance: segment.provenance ? { ...segment.provenance } : segment.provenance,
  };
}

export function cloneSubtitleSnapshot(
  segment: VideoSegment | null,
): S2sSubtitleStateSnapshot | null {
  if (!segment) return null;
  const normalized = normalizeSubtitleTrackState(segment);
  return {
    subtitleTracks: normalized.subtitleTracks?.map((track) => ({
      ...track,
      segments: track.segments.map(cloneSubtitleSegment),
    })),
    activeSubtitleView: normalized.activeSubtitleView
      ? { ...normalized.activeSubtitleView }
      : undefined,
    subtitleCustomChain: normalized.subtitleCustomChain?.map((item) => ({ ...item })),
    subtitleSegments: normalized.subtitleSegments?.map(cloneSubtitleSegment),
  };
}

function restoreSubtitleSnapshot(
  segment: VideoSegment,
  snapshot: S2sSubtitleStateSnapshot,
): VideoSegment {
  return normalizeSubtitleTrackState({
    ...segment,
    subtitleTracks: snapshot.subtitleTracks?.map((track) => ({
      ...track,
      segments: track.segments.map(cloneSubtitleSegment),
    })),
    activeSubtitleView: snapshot.activeSubtitleView
      ? { ...snapshot.activeSubtitleView }
      : undefined,
    subtitleCustomChain: snapshot.subtitleCustomChain?.map((item) => ({ ...item })),
    subtitleSegments: snapshot.subtitleSegments?.map(cloneSubtitleSegment),
  });
}

export function populateEmptyS2sSubtitleTracks(
  segment: VideoSegment,
  sourceSegments: SubtitleSegment[],
  targetSegments: SubtitleSegment[],
  targetLanguage: string,
  options: PopulateS2sSubtitleTracksOptions = {},
): VideoSegment {
  const sourceSegment = options.restoreSnapshot
    ? restoreSubtitleSnapshot(segment, options.restoreSnapshot)
    : segment;
  if (sourceSegments.length === 0 && targetSegments.length === 0) {
    if (options.restoreSnapshot) return sourceSegment;
    return normalizeSubtitleTrackState(sourceSegment);
  }
  const normalized = normalizeSubtitleTrackState(sourceSegment);
  const targetTrackId = getTranslationSubtitleTrackId(targetLanguage);
  const existingOriginalSegments =
    normalized.subtitleTracks?.find((track) => track.id === ORIGINAL_SUBTITLE_TRACK_ID)
      ?.segments ?? [];
  const existingTargetSegments =
    normalized.subtitleTracks?.find((track) => track.id === targetTrackId)
      ?.segments ?? [];
  const sourceBaseSegments = options.baseSourceSegments ?? existingOriginalSegments;
  const targetBaseSegments = options.baseTargetSegments ?? existingTargetSegments;
  const nextSourceSegments = options.preserveExistingOutside
    ? mergeS2sSubtitleSegments(sourceBaseSegments, sourceSegments, options.replacementRanges)
    : replaceS2sSubtitleSegments(sourceSegments);
  const nextTargetSegments = options.preserveExistingOutside
    ? mergeS2sSubtitleSegments(targetBaseSegments, targetSegments, options.replacementRanges)
    : replaceS2sSubtitleSegments(targetSegments);
  const originalTrack: SubtitleTrack = {
    id: ORIGINAL_SUBTITLE_TRACK_ID,
    kind: "original",
    slotLabel: null,
    targetLanguage: null,
    segments: nextSourceSegments,
  };
  const withOriginal = upsertSubtitleTrack(normalized, originalTrack);
  if (targetSegments.length === 0) return withOriginal;
  const withTranslation = upsertSubtitleTrack(withOriginal, {
    id: targetTrackId,
    kind: "translation",
    slotLabel: null,
    targetLanguage,
    segments: nextTargetSegments,
  });
  return setActiveSubtitleTrackView(withTranslation, targetTrackId);
}
