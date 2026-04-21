import type { SubtitleTrack, VideoSegment } from '@/types/video';
import {
  createSubtitleTrackStateFromSegments,
  getActiveSubtitleView,
  getSubtitleTracks,
  normalizeSubtitleTrackState,
} from '@/lib/subtitleTracks';

type SubtitleTrackState = Pick<
  VideoSegment,
  'subtitleTracks' | 'activeSubtitleView' | 'subtitleCustomChain' | 'subtitleSegments'
>;

function cloneSubtitleTrack(track: SubtitleTrack): SubtitleTrack {
  return {
    ...track,
    segments: track.segments.map((segment) => ({
      ...segment,
      style: JSON.parse(JSON.stringify(segment.style)),
    })),
  };
}

function normalizeTrackState(state: SubtitleTrackState): SubtitleTrackState {
  const normalized = normalizeSubtitleTrackState({
    trimStart: 0,
    trimEnd: 0,
    zoomKeyframes: [],
    textSegments: [],
    ...createSubtitleTrackStateFromSegments([]),
    ...state,
  });
  return {
    subtitleTracks: normalized.subtitleTracks,
    activeSubtitleView: normalized.activeSubtitleView,
    subtitleCustomChain: normalized.subtitleCustomChain,
    subtitleSegments: normalized.subtitleSegments,
  };
}

export function projectSubtitleTrackState(
  segment: VideoSegment,
  projectTime: (time: number) => number,
): SubtitleTrackState {
  const normalized = normalizeSubtitleTrackState(segment);
  return normalizeTrackState({
    subtitleTracks: getSubtitleTracks(normalized).map((track) => ({
      ...track,
      segments: track.segments.map((subtitle) => ({
        ...subtitle,
        startTime: projectTime(subtitle.startTime),
        endTime: projectTime(subtitle.endTime),
      })),
    })),
    activeSubtitleView: getActiveSubtitleView(normalized),
    subtitleCustomChain: normalized.subtitleCustomChain ?? createSubtitleTrackStateFromSegments([]).subtitleCustomChain,
  });
}

export function filterSubtitleTrackState(
  segment: VideoSegment,
  overlapsClip: (startTime: number, endTime: number) => boolean,
  toClipTime: (time: number) => number,
): SubtitleTrackState {
  const normalized = normalizeSubtitleTrackState(segment);
  return normalizeTrackState({
    subtitleTracks: getSubtitleTracks(normalized).map((track) => ({
      ...track,
      segments: track.segments
        .filter((subtitle) => overlapsClip(subtitle.startTime, subtitle.endTime))
        .map((subtitle) => ({
          ...subtitle,
          startTime: toClipTime(subtitle.startTime),
          endTime: toClipTime(subtitle.endTime),
        })),
    })),
    activeSubtitleView: getActiveSubtitleView(normalized),
    subtitleCustomChain: normalized.subtitleCustomChain ?? createSubtitleTrackStateFromSegments([]).subtitleCustomChain,
  });
}

function mergeTrackLists(
  baseTracks: readonly SubtitleTrack[],
  incomingTracks: readonly SubtitleTrack[],
  mergeSegments: (baseTrack: SubtitleTrack | null, incomingTrack: SubtitleTrack) => SubtitleTrack,
): SubtitleTrack[] {
  const order = new Map<string, number>();
  const merged = new Map<string, SubtitleTrack>();
  [...baseTracks, ...incomingTracks].forEach((track, index) => {
    if (!order.has(track.id)) {
      order.set(track.id, index);
    }
  });

  for (const track of baseTracks) {
    merged.set(track.id, cloneSubtitleTrack(track));
  }

  for (const track of incomingTracks) {
    const existing = merged.get(track.id) ?? null;
    merged.set(track.id, mergeSegments(existing, track));
  }

  return [...merged.values()].sort(
    (left, right) => (order.get(left.id) ?? 0) - (order.get(right.id) ?? 0),
  );
}

export function appendProjectedSubtitleTrackState(
  baseState: SubtitleTrackState,
  incomingState: SubtitleTrackState,
): SubtitleTrackState {
  const baseTracks = baseState.subtitleTracks ?? [];
  const incomingTracks = incomingState.subtitleTracks ?? [];
  return normalizeTrackState({
    subtitleTracks: mergeTrackLists(baseTracks, incomingTracks, (baseTrack, incomingTrack) => ({
      ...(baseTrack ?? incomingTrack),
      ...incomingTrack,
      segments: [
        ...(baseTrack?.segments ?? []),
        ...cloneSubtitleTrack(incomingTrack).segments,
      ].sort((left, right) => left.startTime - right.startTime),
    })),
    activeSubtitleView: baseState.activeSubtitleView ?? incomingState.activeSubtitleView,
    subtitleCustomChain: baseState.subtitleCustomChain ?? incomingState.subtitleCustomChain,
  });
}

export function replaceProjectedSubtitleTrackState(
  baseState: SubtitleTrackState,
  incomingState: SubtitleTrackState,
  overlapsClip: (startTime: number, endTime: number) => boolean,
): SubtitleTrackState {
  const baseTracks = baseState.subtitleTracks ?? [];
  const incomingTracks = incomingState.subtitleTracks ?? [];
  return normalizeTrackState({
    subtitleTracks: mergeTrackLists(baseTracks, incomingTracks, (baseTrack, incomingTrack) => ({
      ...(baseTrack ?? incomingTrack),
      ...incomingTrack,
      segments: [
        ...((baseTrack?.segments ?? []).filter(
          (subtitle) => !overlapsClip(subtitle.startTime, subtitle.endTime),
        )),
        ...cloneSubtitleTrack(incomingTrack).segments,
      ].sort((left, right) => left.startTime - right.startTime),
    })),
    activeSubtitleView: incomingState.activeSubtitleView ?? baseState.activeSubtitleView,
    subtitleCustomChain: incomingState.subtitleCustomChain ?? baseState.subtitleCustomChain,
  });
}
