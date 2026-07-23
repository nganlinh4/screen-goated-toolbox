import {
  getSubtitleSourceGroupId,
  subtitleOverlapsSourceGroup,
  type SubtitleSourceGroupId,
} from '@/lib/subtitleSourceGroups';
import {
  collectSubtitleIdsForTranslation,
  ensureTranslatedTrack,
  patchSubtitleTrackTexts,
} from '@/lib/subtitleTrackMutations';
import {
  getSubtitleTracks,
  ORIGINAL_SUBTITLE_TRACK_ID,
  setActiveSubtitleTrackView,
} from '@/lib/subtitleTracks';
import type { Translations } from '@/i18n';
import type { ProjectComposition, VideoSegment } from '@/types/video';
import type { BaseAsyncJobStatus } from '@/hooks/asyncJobTypes';
import { localizeMessageKey } from '@/lib/statusFormat';
import { createPersistedSetting } from '@/lib/persistedState';

const SUBTITLE_TRANSLATION_LANGUAGE_KEY =
  'screen-record-subtitle-translation-language-v1';
const SUBTITLE_TRANSLATION_CHUNK_COUNT_KEY =
  'screen-record-subtitle-translation-chunk-count-v2';
const SUBTITLE_TRANSLATION_INSTRUCTIONS_KEY =
  'screen-record-subtitle-translation-instructions-v1';
const SUBTITLE_TRANSLATION_SOURCE_KEY =
  'screen-record-subtitle-translation-source-v1';
const SUBTITLE_TRANSLATION_MODEL_KEY =
  'screen-record-subtitle-translation-model-v1';
const SUBTITLE_TRANSLATION_SMART_FALLBACK_KEY =
  'screen-record-subtitle-translation-smart-fallback-v1';

export const GTX_TRANSLATION_MODEL_ID = 'gtx';

export type SubtitleTranslationSource =
  | 'current'
  | 'all'
  | Exclude<SubtitleSourceGroupId, 'unassigned'>;

export interface SubtitleTranslationResultItem {
  id: string;
  clipId?: string | null;
  translatedText: string;
}

export interface SubtitleTranslationJobStatus extends BaseAsyncJobStatus {
  currentModelId?: string | null;
  currentModelLabel?: string | null;
  currentChunkCount: number;
  currentChunkIndex: number;
  totalChunks: number;
  targetLanguage?: string | null;
  results: SubtitleTranslationResultItem[];
}

export interface SubtitleTranslationCapabilities {
  available: boolean;
  reason?: string | null;
  models: Array<{
    modelId: string;
    modelLabel: string;
    modelName: string;
    provider: string;
    qualityTier?: number | null;
    typicalLatencyMs?: number | null;
    performanceSource?: string | null;
  }>;
}

export interface SubtitleTranslationJobContext {
  targetLanguage: string;
  targetTrackId: string | null;
}

const translationLanguageSetting = createPersistedSetting<string>(
  SUBTITLE_TRANSLATION_LANGUAGE_KEY,
  {
    parse: (raw) => (raw && raw.trim() ? raw : 'en'),
    serialize: (value) => value,
    fallback: 'en',
  },
);

const translationChunkCountSetting = createPersistedSetting<number | null>(
  SUBTITLE_TRANSLATION_CHUNK_COUNT_KEY,
  {
    parse: (raw) => {
      const value = Number(raw);
      if (Number.isFinite(value) && value >= 1) {
        return Math.round(value);
      }
      return null;
    },
    serialize: (value) => (value === null ? null : String(value)),
    fallback: null,
  },
);

const translationInstructionsSetting = createPersistedSetting<string>(
  SUBTITLE_TRANSLATION_INSTRUCTIONS_KEY,
  {
    parse: (raw) => raw ?? '',
    serialize: (value) => value,
    fallback: '',
  },
);

const translationSourceSetting = createPersistedSetting<SubtitleTranslationSource>(
  SUBTITLE_TRANSLATION_SOURCE_KEY,
  {
    parse: (raw) => {
      if (
        raw === 'current' ||
        raw === 'all' ||
        raw === 'video' ||
        raw === 'mic' ||
        raw === 'audio'
      ) {
        return raw as SubtitleTranslationSource;
      }
      if (raw?.startsWith('audio:')) {
        return 'audio';
      }
      return 'current';
    },
    serialize: (value) => value,
    fallback: 'current',
  },
);

const translationModelIdSetting = createPersistedSetting<string>(
  SUBTITLE_TRANSLATION_MODEL_KEY,
  {
    parse: (raw) => (raw && raw.trim() ? raw : GTX_TRANSLATION_MODEL_ID),
    serialize: (value) => value,
    fallback: GTX_TRANSLATION_MODEL_ID,
  },
);

const translationSmartFallbackSetting = createPersistedSetting<boolean>(
  SUBTITLE_TRANSLATION_SMART_FALLBACK_KEY,
  {
    parse: (raw) => raw === 'true',
    serialize: (value) => String(value),
    fallback: false,
  },
);

export function getInitialTranslationLanguage(): string {
  return translationLanguageSetting.getInitial();
}

export function getInitialTranslationChunkCount(): number | null {
  return translationChunkCountSetting.getInitial();
}

export function getInitialTranslationInstructions(): string {
  return translationInstructionsSetting.getInitial();
}

export function getInitialTranslationSource(): SubtitleTranslationSource {
  return translationSourceSetting.getInitial();
}

export function getInitialTranslationModelId(): string {
  return translationModelIdSetting.getInitial();
}

export function getInitialSmartFallback(): boolean {
  return translationSmartFallbackSetting.getInitial();
}

export function persistTranslationLanguage(targetLanguage: string): void {
  translationLanguageSetting.persist(targetLanguage);
}

export function persistTranslationChunkCount(
  chunkCountOverride: number | null,
): void {
  translationChunkCountSetting.persist(chunkCountOverride);
}

export function persistTranslationInstructions(instructions: string): void {
  translationInstructionsSetting.persist(instructions);
}

export function persistTranslationSource(
  translationSource: SubtitleTranslationSource,
): void {
  translationSourceSetting.persist(translationSource);
}

export function persistTranslationModelId(selectedModelId: string): void {
  translationModelIdSetting.persist(selectedModelId);
}

export function persistTranslationSmartFallback(smartFallback: boolean): void {
  translationSmartFallbackSetting.persist(smartFallback);
}

export function localizeStatus(
  t: Translations,
  status: Pick<
    SubtitleTranslationJobStatus,
    'message' | 'messageKey' | 'messageParams'
  > | null,
) {
  if (!status) return null;
  return localizeMessageKey(t, status);
}

export function buildTranslationItems(
  segment: VideoSegment | null,
  selectedSubtitleIds: readonly string[],
  editingSubtitleId: string | null,
  source: SubtitleTranslationSource,
) {
  if (!segment) return [];
  const targetIds = new Set(
    collectSubtitleIdsForTranslation(
      segment,
      selectedSubtitleIds,
      editingSubtitleId,
    ),
  );
  const tracks = getSubtitleTracks(segment);
  const originalTrack = tracks.find(
    (track) => track.id === ORIGINAL_SUBTITLE_TRACK_ID,
  );
  return (originalTrack?.segments ?? [])
    .filter((subtitle) => targetIds.has(subtitle.id))
    .filter(
      (subtitle) =>
        source === 'current' ||
        source === 'all' ||
        subtitleOverlapsSourceGroup(subtitle, source),
    )
    .sort((left, right) => {
      if (source !== 'audio') return left.startTime - right.startTime;
      const groupCompare = getSubtitleSourceGroupId(left).localeCompare(
        getSubtitleSourceGroupId(right),
      );
      return groupCompare || left.startTime - right.startTime;
    })
    .map((subtitle) => ({
      id: subtitle.id,
      clipId: 'root',
      text: subtitle.text,
      sourceGroupId:
        source === 'current' || source === 'all' || source === 'audio'
          ? getSubtitleSourceGroupId(subtitle)
          : source,
      sourceName:
        subtitle.sourceGroup?.sourceName ?? subtitle.provenance?.sourceName ?? null,
    }));
}

export function buildCompositionTranslationItems(
  composition: ProjectComposition,
  selectedSubtitleIds: readonly string[],
  editingSubtitleId: string | null,
  source: SubtitleTranslationSource,
) {
  const targetIds = new Set<string>(
    selectedSubtitleIds.length > 0
      ? selectedSubtitleIds
      : editingSubtitleId
        ? [editingSubtitleId]
        : composition.clips.flatMap((clip) =>
            (
              getSubtitleTracks(clip.segment).find(
                (track) => track.id === ORIGINAL_SUBTITLE_TRACK_ID,
              )?.segments ?? []
            ).map((subtitle) => subtitle.id),
          ),
  );

  return composition.clips.flatMap((clip) => {
    const originalTrack = getSubtitleTracks(clip.segment).find(
      (track) => track.id === ORIGINAL_SUBTITLE_TRACK_ID,
    );
    return (originalTrack?.segments ?? [])
      .filter((subtitle) => targetIds.has(subtitle.id))
      .filter(
        (subtitle) =>
          source === 'current' ||
          source === 'all' ||
          subtitleOverlapsSourceGroup(subtitle, source),
      )
      .sort((left, right) => {
        if (source !== 'audio') return left.startTime - right.startTime;
        const groupCompare = getSubtitleSourceGroupId(left).localeCompare(
          getSubtitleSourceGroupId(right),
        );
        return groupCompare || left.startTime - right.startTime;
      })
      .map((subtitle) => ({
        id: subtitle.id,
        clipId: clip.id,
        text: subtitle.text,
        sourceGroupId:
          source === 'current' || source === 'all' || source === 'audio'
            ? getSubtitleSourceGroupId(subtitle)
            : source,
        sourceName:
          subtitle.sourceGroup?.sourceName ??
          subtitle.provenance?.sourceName ??
          null,
      }));
  });
}

export function clampTranslationChunkCount(value: number, itemCount: number) {
  return Math.max(1, Math.min(Math.max(1, itemCount), Math.round(value)));
}

export function suggestTranslationChunkCount(items: Array<{ text: string }>) {
  if (items.length <= 12) return 1;
  const totalChars = items.reduce(
    (total, item) => total + item.text.trim().length,
    0,
  );
  return clampTranslationChunkCount(
    Math.max(Math.ceil(items.length / 24), Math.ceil(totalChars / 2600), 1),
    items.length,
  );
}

export function buildTranslationChunkPreview(
  items: Array<{ id: string; clipId?: string | null }>,
  chunkCount: number,
) {
  const safeChunkCount = clampTranslationChunkCount(chunkCount, items.length);
  const groups: Record<string, number> = {};
  for (let chunkIndex = 0; chunkIndex < safeChunkCount; chunkIndex += 1) {
    const start = Math.floor((chunkIndex * items.length) / safeChunkCount);
    const end = Math.floor(((chunkIndex + 1) * items.length) / safeChunkCount);
    for (const item of items.slice(start, end)) {
      groups[item.id] = chunkIndex;
    }
  }
  return {
    groups,
    groupCount: safeChunkCount,
  };
}

export function applyTranslationResultsToSegment(
  segment: VideoSegment,
  results: SubtitleTranslationResultItem[],
  context: SubtitleTranslationJobContext,
) {
  const patches = new Map(
    results.map((result) => [result.id, result.translatedText]),
  );
  const ensured = ensureTranslatedTrack(
    segment,
    context.targetLanguage,
    context.targetTrackId,
  );
  return setActiveSubtitleTrackView(
    patchSubtitleTrackTexts(ensured.segment, ensured.track.id, patches),
    ensured.track.id,
  );
}

export function updateSubtitleViewAcrossComposition(
  composition: ProjectComposition,
  updater: (segment: VideoSegment) => VideoSegment,
) {
  return {
    ...composition,
    clips: composition.clips.map((clip) => ({
      ...clip,
      segment: updater(clip.segment),
    })),
    globalSegment: composition.globalSegment
      ? updater(composition.globalSegment)
      : composition.globalSegment,
  };
}
