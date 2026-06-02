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

export interface SubtitleTranslationJobStatus {
  state: 'queued' | 'running' | 'completed' | 'cancelled' | 'error';
  message: string;
  messageKey?: string | null;
  messageParams?: Record<string, string> | null;
  progress: number;
  currentModelId?: string | null;
  currentModelLabel?: string | null;
  currentChunkCount: number;
  currentChunkIndex: number;
  totalChunks: number;
  targetLanguage?: string | null;
  results: SubtitleTranslationResultItem[];
  error?: string | null;
}

export interface SubtitleTranslationCapabilities {
  available: boolean;
  reason?: string | null;
  models: Array<{
    modelId: string;
    modelLabel: string;
    modelName: string;
    provider: string;
  }>;
}

export interface SubtitleTranslationJobContext {
  targetLanguage: string;
  targetTrackId: string | null;
}

export function getInitialTranslationLanguage(): string {
  try {
    const raw = localStorage.getItem(SUBTITLE_TRANSLATION_LANGUAGE_KEY);
    if (raw && raw.trim()) {
      return raw;
    }
  } catch {
    // ignore persistence failures
  }
  return 'en';
}

export function getInitialTranslationChunkCount(): number | null {
  try {
    const raw = Number(
      localStorage.getItem(SUBTITLE_TRANSLATION_CHUNK_COUNT_KEY),
    );
    if (Number.isFinite(raw) && raw >= 1) {
      return Math.round(raw);
    }
  } catch {
    // ignore persistence failures
  }
  return null;
}

export function getInitialTranslationInstructions(): string {
  try {
    return localStorage.getItem(SUBTITLE_TRANSLATION_INSTRUCTIONS_KEY) ?? '';
  } catch {
    // ignore persistence failures
  }
  return '';
}

export function getInitialTranslationSource(): SubtitleTranslationSource {
  try {
    const raw = localStorage.getItem(SUBTITLE_TRANSLATION_SOURCE_KEY);
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
  } catch {
    // ignore persistence failures
  }
  return 'current';
}

export function getInitialTranslationModelId(): string {
  try {
    const raw = localStorage.getItem(SUBTITLE_TRANSLATION_MODEL_KEY);
    if (raw && raw.trim()) {
      return raw;
    }
  } catch {
    // ignore persistence failures
  }
  return GTX_TRANSLATION_MODEL_ID;
}

export function getInitialSmartFallback(): boolean {
  try {
    return (
      localStorage.getItem(SUBTITLE_TRANSLATION_SMART_FALLBACK_KEY) === 'true'
    );
  } catch {
    // ignore persistence failures
  }
  return false;
}

export function persistTranslationLanguage(targetLanguage: string): void {
  localStorage.setItem(SUBTITLE_TRANSLATION_LANGUAGE_KEY, targetLanguage);
}

export function persistTranslationChunkCount(
  chunkCountOverride: number | null,
): void {
  if (chunkCountOverride === null) {
    localStorage.removeItem(SUBTITLE_TRANSLATION_CHUNK_COUNT_KEY);
    return;
  }
  localStorage.setItem(
    SUBTITLE_TRANSLATION_CHUNK_COUNT_KEY,
    String(chunkCountOverride),
  );
}

export function persistTranslationInstructions(instructions: string): void {
  localStorage.setItem(SUBTITLE_TRANSLATION_INSTRUCTIONS_KEY, instructions);
}

export function persistTranslationSource(
  translationSource: SubtitleTranslationSource,
): void {
  localStorage.setItem(SUBTITLE_TRANSLATION_SOURCE_KEY, translationSource);
}

export function persistTranslationModelId(selectedModelId: string): void {
  localStorage.setItem(SUBTITLE_TRANSLATION_MODEL_KEY, selectedModelId);
}

export function persistTranslationSmartFallback(smartFallback: boolean): void {
  localStorage.setItem(
    SUBTITLE_TRANSLATION_SMART_FALLBACK_KEY,
    String(smartFallback),
  );
}

function formatTemplate(
  template: string,
  params?: Record<string, string> | null,
) {
  let formatted = template;
  for (const [key, value] of Object.entries(params ?? {})) {
    formatted = formatted.split(`{${key}}`).join(value);
  }
  return formatted;
}

export function localizeStatus(
  t: Translations,
  status: Pick<
    SubtitleTranslationJobStatus,
    'message' | 'messageKey' | 'messageParams'
  > | null,
) {
  if (!status) return null;
  const key = status.messageKey;
  if (key && key in t) {
    return formatTemplate(
      t[key as keyof Translations] as string,
      status.messageParams,
    );
  }
  return status.message;
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
