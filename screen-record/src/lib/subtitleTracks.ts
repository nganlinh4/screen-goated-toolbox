import type {
  SubtitleChainItem,
  SubtitleSegment,
  SubtitleTrack,
  SubtitleViewState,
  VideoSegment,
} from '@/types/video';
import { getSubtitleLanguageLabel } from '@/lib/subtitleLanguageOptions';

export const ORIGINAL_SUBTITLE_TRACK_ID = 'subtitle-track-original';

export function buildSubtitleTrackLabel(index: number): string {
  let value = Math.max(0, Math.floor(index));
  let label = '';
  do {
    label = String.fromCharCode(65 + (value % 26)) + label;
    value = Math.floor(value / 26) - 1;
  } while (value >= 0);
  return label;
}

function sanitizeTrackIdPart(value: string): string {
  return value
    .trim()
    .toLocaleLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '') || 'translation';
}

export function getTranslationSubtitleTrackId(targetLanguage: string): string {
  return `subtitle-track-translation-${sanitizeTrackIdPart(targetLanguage)}`;
}

export function buildDefaultSubtitleCustomChain(): SubtitleChainItem[] {
  return [{ type: 'track', trackId: ORIGINAL_SUBTITLE_TRACK_ID }];
}

export function createSubtitleTrackStateFromSegments(
  segments: readonly SubtitleSegment[] = [],
): Pick<VideoSegment, 'subtitleTracks' | 'activeSubtitleView' | 'subtitleCustomChain' | 'subtitleSegments'> {
  const originalTrack: SubtitleTrack = {
    id: ORIGINAL_SUBTITLE_TRACK_ID,
    kind: 'original',
    slotLabel: null,
    targetLanguage: null,
    segments: cloneSubtitleSegments(segments),
  };
  return {
    subtitleTracks: [originalTrack],
    activeSubtitleView: {
      kind: 'track',
      trackId: ORIGINAL_SUBTITLE_TRACK_ID,
    },
    subtitleCustomChain: buildDefaultSubtitleCustomChain(),
    subtitleSegments: cloneSubtitleSegments(originalTrack.segments),
  };
}

function cloneSubtitleSegment(segment: SubtitleSegment): SubtitleSegment {
  return {
    ...segment,
    style: JSON.parse(JSON.stringify(segment.style)),
  };
}

function cloneSubtitleSegments(segments: readonly SubtitleSegment[]): SubtitleSegment[] {
  return segments.map(cloneSubtitleSegment);
}

function sortSubtitleTracks(tracks: readonly SubtitleTrack[]): SubtitleTrack[] {
  return [...tracks].sort((left, right) => {
    if (left.kind === 'original' && right.kind !== 'original') return -1;
    if (left.kind !== 'original' && right.kind === 'original') return 1;
    const leftLabel = left.kind === 'translation'
      ? getSubtitleTrackLabel(left)
      : '';
    const rightLabel = right.kind === 'translation'
      ? getSubtitleTrackLabel(right)
      : '';
    return leftLabel.localeCompare(rightLabel);
  });
}

function ensureOriginalTrack(tracks: readonly SubtitleTrack[], legacySegments: readonly SubtitleSegment[]): SubtitleTrack[] {
  if (tracks.some((track) => track.id === ORIGINAL_SUBTITLE_TRACK_ID)) {
    return [...tracks];
  }
  return [
    {
      id: ORIGINAL_SUBTITLE_TRACK_ID,
      kind: 'original',
      slotLabel: null,
      targetLanguage: null,
      segments: cloneSubtitleSegments(legacySegments),
    },
    ...tracks,
  ];
}

function normalizeTrackSegments(track: SubtitleTrack): SubtitleTrack {
  return {
    ...track,
    segments: cloneSubtitleSegments(track.segments ?? []),
  };
}

function normalizeTracks(segment: VideoSegment): SubtitleTrack[] {
  const legacySegments = Array.isArray(segment.subtitleSegments)
    ? segment.subtitleSegments
    : [];
  const sourceTracks = Array.isArray(segment.subtitleTracks)
    ? segment.subtitleTracks.map(normalizeTrackSegments)
    : [];
  return sortSubtitleTracks(ensureOriginalTrack(sourceTracks, legacySegments));
}

function normalizeActiveView(tracks: readonly SubtitleTrack[], activeView?: SubtitleViewState): SubtitleViewState {
  if (activeView?.kind === 'custom') {
    return { kind: 'custom' };
  }
  const requestedTrackId = activeView?.trackId;
  const resolvedTrack = tracks.find((track) => track.id === requestedTrackId)
    ?? tracks.find((track) => track.id === ORIGINAL_SUBTITLE_TRACK_ID)
    ?? tracks[0]
    ?? null;
  return {
    kind: 'track',
    trackId: resolvedTrack?.id ?? ORIGINAL_SUBTITLE_TRACK_ID,
  };
}

function normalizeCustomChain(
  tracks: readonly SubtitleTrack[],
  chain: readonly SubtitleChainItem[] | undefined,
): SubtitleChainItem[] {
  if (!Array.isArray(chain) || chain.length === 0) {
    return buildDefaultSubtitleCustomChain();
  }
  const availableTrackIds = new Set(tracks.map((track) => track.id));
  const filtered = chain.filter((item) =>
    item.type === 'delimiter' || availableTrackIds.has(item.trackId),
  );
  return filtered.length > 0 ? filtered : buildDefaultSubtitleCustomChain();
}

function getTrackTextForSegment(
  track: SubtitleTrack | undefined,
  segmentId: string,
  fallbackText: string,
): string {
  const value = track?.segments.find((segment) => segment.id === segmentId)?.text?.trim();
  return value && value.length > 0 ? value : fallbackText;
}

function buildCustomSubtitleSegments(
  tracks: readonly SubtitleTrack[],
  chain: readonly SubtitleChainItem[],
): SubtitleSegment[] {
  const originalTrack = tracks.find((track) => track.id === ORIGINAL_SUBTITLE_TRACK_ID) ?? tracks[0];
  if (!originalTrack) return [];
  return originalTrack.segments.map((segment) => {
    const originalText = segment.text;
    let text = '';
    for (const item of chain) {
      if (item.type === 'delimiter') {
        text += item.value ?? '';
        continue;
      }
      const track = tracks.find((entry) => entry.id === item.trackId);
      text += getTrackTextForSegment(track, segment.id, originalText);
    }
    return {
      ...cloneSubtitleSegment(segment),
      text,
    };
  });
}

export function normalizeSubtitleTrackState(segment: VideoSegment): VideoSegment {
  const subtitleTracks = normalizeTracks(segment);
  const activeSubtitleView = normalizeActiveView(subtitleTracks, segment.activeSubtitleView);
  const subtitleCustomChain = normalizeCustomChain(subtitleTracks, segment.subtitleCustomChain);
  const activeTrack = subtitleTracks.find((track) => track.id === activeSubtitleView.trackId)
    ?? subtitleTracks[0]
    ?? null;
  const subtitleSegments = activeSubtitleView.kind === 'custom'
    ? buildCustomSubtitleSegments(subtitleTracks, subtitleCustomChain)
    : cloneSubtitleSegments(activeTrack?.segments ?? []);
  return {
    ...segment,
    subtitleTracks,
    activeSubtitleView,
    subtitleCustomChain,
    subtitleSegments,
  };
}

export function getSubtitleTracks(segment: VideoSegment | null | undefined): SubtitleTrack[] {
  if (!segment) return [];
  return normalizeSubtitleTrackState(segment).subtitleTracks ?? [];
}

export function getOriginalSubtitleTrack(segment: VideoSegment | null | undefined): SubtitleTrack | null {
  return getSubtitleTracks(segment).find((track) => track.id === ORIGINAL_SUBTITLE_TRACK_ID) ?? null;
}

export function getSubtitleTrack(
  segment: VideoSegment | null | undefined,
  trackId: string | null | undefined,
): SubtitleTrack | null {
  if (!trackId) return null;
  return getSubtitleTracks(segment).find((track) => track.id === trackId) ?? null;
}

export function findTranslationTrackByLanguage(
  segment: VideoSegment | null | undefined,
  targetLanguage: string | null | undefined,
): SubtitleTrack | null {
  if (!targetLanguage) return null;
  return getSubtitleTracks(segment).find(
    (track) => track.kind === 'translation' && track.targetLanguage === targetLanguage,
  ) ?? null;
}

export function getActiveSubtitleView(segment: VideoSegment | null | undefined): SubtitleViewState {
  if (!segment) {
    return { kind: 'track', trackId: ORIGINAL_SUBTITLE_TRACK_ID };
  }
  return normalizeSubtitleTrackState(segment).activeSubtitleView ?? {
    kind: 'track',
    trackId: ORIGINAL_SUBTITLE_TRACK_ID,
  };
}

export function getActiveSubtitleTrack(segment: VideoSegment | null | undefined): SubtitleTrack | null {
  const activeView = getActiveSubtitleView(segment);
  if (activeView.kind !== 'track') return null;
  return getSubtitleTrack(segment, activeView.trackId) ?? getOriginalSubtitleTrack(segment);
}

export function getVisibleSubtitleSegments(segment: VideoSegment | null | undefined): SubtitleSegment[] {
  if (!segment) return [];
  return normalizeSubtitleTrackState(segment).subtitleSegments ?? [];
}

export function setActiveSubtitleTrackView(segment: VideoSegment, trackId: string): VideoSegment {
  return normalizeSubtitleTrackState({
    ...segment,
    activeSubtitleView: {
      kind: 'track',
      trackId,
    },
  });
}

export function setActiveSubtitleCustomView(segment: VideoSegment): VideoSegment {
  return normalizeSubtitleTrackState({
    ...segment,
    activeSubtitleView: { kind: 'custom' },
  });
}

export function updateSubtitleTrack(
  segment: VideoSegment,
  trackId: string,
  updater: (track: SubtitleTrack) => SubtitleTrack,
): VideoSegment {
  const normalized = normalizeSubtitleTrackState(segment);
  return normalizeSubtitleTrackState({
    ...normalized,
    subtitleTracks: normalized.subtitleTracks?.map((track) =>
      track.id === trackId ? updater(track) : track,
    ),
  });
}

export function updateAllSubtitleTracks(
  segment: VideoSegment,
  updater: (track: SubtitleTrack) => SubtitleTrack,
): VideoSegment {
  const normalized = normalizeSubtitleTrackState(segment);
  return normalizeSubtitleTrackState({
    ...normalized,
    subtitleTracks: normalized.subtitleTracks?.map(updater),
  });
}

export function upsertSubtitleTrack(segment: VideoSegment, nextTrack: SubtitleTrack): VideoSegment {
  const normalized = normalizeSubtitleTrackState(segment);
  const currentTracks = normalized.subtitleTracks ?? [];
  const existingIndex = currentTracks.findIndex((track) => track.id === nextTrack.id);
  const subtitleTracks = existingIndex >= 0
    ? currentTracks.map((track) => (track.id === nextTrack.id ? normalizeTrackSegments(nextTrack) : track))
    : sortSubtitleTracks([...currentTracks, normalizeTrackSegments(nextTrack)]);
  return normalizeSubtitleTrackState({
    ...normalized,
    subtitleTracks,
  });
}

export function removeSubtitleTrack(segment: VideoSegment, trackId: string): VideoSegment {
  if (trackId === ORIGINAL_SUBTITLE_TRACK_ID) {
    return normalizeSubtitleTrackState(segment);
  }
  const normalized = normalizeSubtitleTrackState(segment);
  return normalizeSubtitleTrackState({
    ...normalized,
    subtitleTracks: (normalized.subtitleTracks ?? []).filter((track) => track.id !== trackId),
  });
}

export function setSubtitleCustomChain(
  segment: VideoSegment,
  subtitleCustomChain: SubtitleChainItem[],
): VideoSegment {
  return normalizeSubtitleTrackState({
    ...segment,
    subtitleCustomChain,
  });
}

export function getNextTranslationSlotLabel(segment: VideoSegment | null | undefined): string {
  const translationCount = getSubtitleTracks(segment).filter((track) => track.kind === 'translation').length;
  return buildSubtitleTrackLabel(translationCount);
}

export function createTranslationTrack(
  segment: VideoSegment,
  targetLanguage: string,
  slotLabel?: string | null,
): SubtitleTrack {
  const originalTrack = getOriginalSubtitleTrack(segment);
  return {
    id: getTranslationSubtitleTrackId(targetLanguage),
    kind: 'translation',
    slotLabel: slotLabel ?? null,
    targetLanguage,
    segments: cloneSubtitleSegments(originalTrack?.segments ?? []),
  };
}

export function getSubtitleTrackLabel(track: SubtitleTrack): string {
  if (track.kind === 'original') return 'Original';
  if (track.targetLanguage) {
    return getSubtitleLanguageLabel(track.targetLanguage);
  }
  return 'Translation';
}
