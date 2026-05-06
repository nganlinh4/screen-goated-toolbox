import { startTransition, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@/lib/ipc';
import {
  getEffectiveCompositionMode,
  updateCompositionClip,
} from '@/lib/projectComposition';
import {
  buildSequenceTimeline,
  getSequenceClipById,
  mergeCompositionSegmentsToSequence,
  replaceSequenceClipSegmentInGlobal,
} from '@/lib/sequenceTimeline';
import {
  buildSubtitleGenerationPlan,
  type AudioSubtitleClipTransform,
  type SubtitleGenerationIndicator,
  type SubtitleSource,
} from '@/lib/subtitleGenerationPlan';
import { defaultSubtitleStyle } from '@/lib/subtitleDefaults';
import {
  type TrackSelectionRange,
} from '@/lib/timelineSegmentSelection';
import type { Translations } from '@/i18n';
import type { ProjectComposition, SubtitleSegment, VideoSegment } from '@/types/video';
import {
  clearDerivedSubtitleTracks,
  mergePartialOriginalSubtitleSegments,
  replaceOriginalSubtitleSegments,
  replaceAudioSubtitlesOnOriginalTrack,
} from '@/lib/subtitleTrackMutations';
import {
  getSubtitleTracks,
  getVisibleSubtitleSegments,
  ORIGINAL_SUBTITLE_TRACK_ID,
  setActiveSubtitleTrackView,
} from '@/lib/subtitleTracks';
import { DEFAULT_GEMINI_SUBTITLE_PROMPT } from '@/lib/geminiSubtitlePrompt';
import { getSubtitleLanguageOptionsForMethod } from '@/lib/subtitleLanguageOptions';
import { markFrontendPerfEvent } from '@/lib/frontendPerfDiagnostics';
import type { PersistOptions } from '@/hooks/useSequenceComposition';

export type SubtitleMethod =
  | 'groq-whisper-accurate'
  | 'groq-whisper-large-v3-turbo'
  | 'gemini-3-1-flash-lite'
  | 'gemini-3-flash-preview'
  | 'qwen-local-0-6b'
  | 'qwen-local-1-7b'
  | 'parakeet-tdt-0-6b-v3';

const DEFAULT_SUBTITLE_METHOD_CAPABILITIES: Array<{
  method: SubtitleMethod;
  available: boolean;
  reason?: string | null;
}> = [
  { method: 'groq-whisper-accurate', available: true, reason: null },
  { method: 'groq-whisper-large-v3-turbo', available: true, reason: null },
  { method: 'gemini-3-1-flash-lite', available: false, reason: null },
  { method: 'gemini-3-flash-preview', available: false, reason: null },
  { method: 'qwen-local-0-6b', available: false, reason: null },
  { method: 'qwen-local-1-7b', available: false, reason: null },
  { method: 'parakeet-tdt-0-6b-v3', available: false, reason: null },
];

const SUBTITLE_SOURCE_KEY = 'screen-record-subtitle-source-v1';
const SUBTITLE_METHOD_KEY = 'screen-record-subtitle-method-v1';
const SUBTITLE_LANGUAGE_HINT_KEY = 'screen-record-subtitle-language-hint-v1';
const SUBTITLE_GEMINI_PROMPT_KEY = 'screen-record-subtitle-gemini-prompt-v1';
const SUBTITLE_GROQ_VOCABULARY_KEY = 'screen-record-subtitle-groq-vocabulary-v1';
const SUBTITLE_PARTIAL_APPLY_INTERVAL_MS = 2500;
const SUBTITLE_PARTIAL_TEXT_REFRESH_MS = 5000;
const SUBTITLE_APPLY_PERF_LOG_INTERVAL_MS = 1000;

function isSubtitleSource(value: string | null): value is SubtitleSource {
  return value === 'video'
    || value === 'mic'
    || value === 'audio'
    || value?.startsWith('audio:') === true;
}

function normalizeLegacySubtitleSource(value: string | null): string | null {
  if (value === 'music') return 'audio';
  if (value?.startsWith('music:')) return `audio:${value.slice('music:'.length)}`;
  return value;
}

function getInitialSubtitleSource(): SubtitleSource {
  try {
    const raw = normalizeLegacySubtitleSource(localStorage.getItem(SUBTITLE_SOURCE_KEY));
    if (isSubtitleSource(raw)) {
      return raw;
    }
  } catch {
    // ignore persistence failures
  }
  return 'video';
}

function isSubtitleMethod(value: string): value is SubtitleMethod {
  return (
    value === 'groq-whisper-accurate' ||
    value === 'groq-whisper-large-v3-turbo' ||
    value === 'gemini-3-1-flash-lite' ||
    value === 'gemini-3-flash-preview' ||
    value === 'qwen-local-0-6b' ||
    value === 'qwen-local-1-7b' ||
    value === 'parakeet-tdt-0-6b-v3'
  );
}

function normalizeStoredSubtitleMethod(value: string | null): SubtitleMethod | null {
  return value && isSubtitleMethod(value) ? value : null;
}

function isQwenLocalSubtitleMethod(method: SubtitleMethod) {
  return method === 'qwen-local-0-6b' || method === 'qwen-local-1-7b';
}

function isParakeetTdtSubtitleMethod(method: SubtitleMethod) {
  return method === 'parakeet-tdt-0-6b-v3';
}

function getInitialSubtitleMethod(): SubtitleMethod {
  try {
    const normalized = normalizeStoredSubtitleMethod(localStorage.getItem(SUBTITLE_METHOD_KEY));
    if (normalized) {
      return normalized;
    }
  } catch {
    // ignore persistence failures
  }
  return 'groq-whisper-accurate';
}

function getInitialSubtitleLanguageHint(): string {
  try {
    const raw = localStorage.getItem(SUBTITLE_LANGUAGE_HINT_KEY);
    if (raw && raw.trim()) {
      return raw;
    }
  } catch {
    // ignore persistence failures
  }
  return 'auto';
}

function getInitialGeminiPrompt(): string {
  try {
    const storedPrompt = localStorage.getItem(SUBTITLE_GEMINI_PROMPT_KEY);
    return storedPrompt?.trim() ? storedPrompt : DEFAULT_GEMINI_SUBTITLE_PROMPT;
  } catch {
    // ignore persistence failures
  }
  return DEFAULT_GEMINI_SUBTITLE_PROMPT;
}

function getInitialGroqVocabulary(): string[] {
  try {
    const parsed = JSON.parse(localStorage.getItem(SUBTITLE_GROQ_VOCABULARY_KEY) ?? '[]');
    if (Array.isArray(parsed)) {
      return parsed
        .filter((entry): entry is string => typeof entry === 'string')
        .map((entry) => entry.trim())
        .filter(Boolean);
    }
  } catch {
    // ignore persistence failures
  }
  return [];
}

interface SubtitleClipResult {
  clipId: string;
  isPartial: boolean;
  segments: Array<{ startTime: number; endTime: number; text: string }>;
}

interface SubtitleSkippedClip {
  clipId: string;
  reason: string;
}

interface SubtitleMethodCapability {
  method: SubtitleMethod;
  available: boolean;
  reason?: string | null;
}

interface SubtitleGenerationCapabilities {
  methods: SubtitleMethodCapability[];
}

interface PrepareQwenLocalResult {
  available: boolean;
  startedDownloads: boolean;
  reason?: string | null;
}

interface PrepareParakeetTdtResult {
  available: boolean;
  startedDownloads: boolean;
  reason?: string | null;
}

interface SubtitleJobStatus {
  state: 'queued' | 'running' | 'completed' | 'cancelled' | 'error';
  message: string;
  messageKey?: string | null;
  messageParams?: Record<string, string> | null;
  progress: number;
  activeClipId?: string | null;
  totalClips: number;
  completedClips: number;
  resultsRevision: number;
  results: SubtitleClipResult[];
  skipped: SubtitleSkippedClip[];
  error?: string | null;
}

type SubtitleJobViewStatus = Omit<SubtitleJobStatus, 'results'>;

interface SubtitleJobContext {
  replacementRangesByClip: Record<
    string,
    Array<{ startTime: number; endTime: number }>
  >;
  indicator: SubtitleGenerationIndicator;
  sourceTypeForNative: 'video' | 'mic' | 'audio';
  clipTransformsByClip: Record<string, AudioSubtitleClipTransform>;
}

interface UseSubtitleGenerationParams {
  t: Translations;
  projectResetKey?: string | null;
  segment: VideoSegment | null;
  setSegment: (
    segment:
      | VideoSegment
      | null
      | ((prev: VideoSegment | null) => VideoSegment | null),
    withHistory?: boolean,
  ) => void;
  composition: ProjectComposition | null;
  setComposition: (
    composition:
      | ProjectComposition
      | null
      | ((prev: ProjectComposition | null) => ProjectComposition | null),
  ) => void;
  activeClipId: string | null | undefined;
  currentRawVideoPath: string;
  currentRawMicAudioPath: string;
  duration: number;
  setActivePanel: (
    panel: 'zoom' | 'background' | 'cursor' | 'text' | 'subtitles',
  ) => void;
  persistProject?: (opts?: PersistOptions) => Promise<void>;
}

function buildSubtitleId(
  clipId: string,
  entry: { startTime: number; endTime: number; text: string },
  _index: number,
) {
  return `subtitle-${clipId}-${Math.round(entry.startTime * 1000)}`;
}

function formatTemplate(template: string, params?: Record<string, string> | null) {
  let formatted = template;
  for (const [key, value] of Object.entries(params ?? {})) {
    formatted = formatted.split(`{${key}}`).join(value);
  }
  return formatted;
}

function localizeSubtitleStatus(
  t: Translations,
  status: Pick<SubtitleJobViewStatus, 'message' | 'messageKey' | 'messageParams'> | null,
) {
  if (!status) {
    return null;
  }
  const key = status.messageKey;
  if (key && key in t) {
    return formatTemplate(t[key as keyof Translations] as string, status.messageParams);
  }
  return status.message;
}

function stripSubtitleJobResults(status: SubtitleJobStatus): SubtitleJobViewStatus {
  const { results: _results, ...viewStatus } = status;
  return viewStatus;
}

function buildSubtitleStatusViewKey(status: SubtitleJobViewStatus): string {
  return [
    status.state,
    status.messageKey ?? '',
    status.message,
    status.progress.toFixed(4),
    status.activeClipId ?? '',
    status.totalClips,
    status.completedClips,
    status.skipped.length,
    status.error ?? '',
    JSON.stringify(status.messageParams ?? {}),
  ].join('|');
}

function buildAppliedResultsKey(results: SubtitleClipResult[]): string {
  return results.map((result) => [
    result.clipId,
    result.isPartial ? 'p' : 'f',
    String(result.segments.length),
    result.segments.map((segment, index) => {
      const isLastPartial = result.isPartial && index === result.segments.length - 1;
      return [
        Math.round(segment.startTime * 100),
        isLastPartial ? 'live' : Math.round(segment.endTime * 100),
        segment.text,
      ].join(':');
    }).join('|'),
  ].join('/')).join('||');
}

function hasFinalSubtitleResult(results: SubtitleClipResult[]) {
  return results.some((result) => !result.isPartial);
}

function summarizeSubtitleRanges(
  segments: ReadonlyArray<{ startTime: number; endTime: number; text: string }>,
) {
  const first = segments[0];
  const last = segments[segments.length - 1];
  const invalid = segments.filter((subtitle) => (
    !Number.isFinite(subtitle.startTime)
    || !Number.isFinite(subtitle.endTime)
    || subtitle.endTime <= subtitle.startTime
  )).length;
  const emptyText = segments.filter((subtitle) => !subtitle.text.trim()).length;
  const maxEnd = segments.reduce((max, subtitle) => Math.max(max, subtitle.endTime), 0);
  return [
    `count=${segments.length}`,
    `range=${first ? `${first.startTime.toFixed(2)}-${last?.endTime.toFixed(2)}` : 'none'}`,
    `maxEnd=${maxEnd.toFixed(2)}`,
    `invalid=${invalid}`,
    `empty=${emptyText}`,
    `first="${(first?.text ?? '').slice(0, 32)}"`,
    `last="${(last?.text ?? '').slice(0, 32)}"`,
  ].join(' ');
}

function logSubtitleApplyDiagnostics(
  phase: string,
  result: SubtitleClipResult,
  before: VideoSegment | null,
  after: VideoSegment | null,
  replacementRanges: ReadonlyArray<Pick<TrackSelectionRange, 'startTime' | 'endTime'>>,
) {
  const beforeVisible = before ? getVisibleSubtitleSegments(before) : [];
  const afterVisible = after ? getVisibleSubtitleSegments(after) : [];
  const afterTracks = after ? getSubtitleTracks(after) : [];
  const activeView = after?.activeSubtitleView;
  const trackSummary = afterTracks
    .map((track) => `${track.id}:${track.segments.length}`)
    .join(',');
  const replacementSummary = replacementRanges
    .map((range) => `${range.startTime.toFixed(2)}-${range.endTime.toFixed(2)}`)
    .join(',');
  console.log(
    `[SubtitleGen][Diag][${phase}] clip=${result.clipId} partial=${result.isPartial ? 1 : 0} `
    + `incoming(${summarizeSubtitleRanges(result.segments)}) `
    + `visibleBefore=${beforeVisible.length} visibleAfter=${afterVisible.length} `
    + `active=${activeView?.kind ?? 'none'}:${activeView?.trackId ?? 'none'} `
    + `tracks=${trackSummary || 'none'} replacement=${replacementSummary || 'all'}`,
  );
}

function coalesceSubtitleClipResults(
  current: SubtitleClipResult[],
  incoming: SubtitleClipResult[],
) {
  const byClipId = new Map(current.map((result) => [result.clipId, result]));
  for (const result of incoming) {
    const existing = byClipId.get(result.clipId);
    if (!existing || !result.isPartial || existing.isPartial) {
      byClipId.set(result.clipId, result);
    }
  }
  return Array.from(byClipId.values());
}

function partialApplySignature(result: SubtitleClipResult) {
  const tail = result.segments[result.segments.length - 1] ?? null;
  return [
    result.clipId,
    result.segments.length,
    tail ? Math.round(tail.startTime * 10) : 'none',
  ].join(':');
}

function buildInsertedSubtitle(
  result: SubtitleClipResult,
  entry: { startTime: number; endTime: number; text: string },
  index: number,
  subtitleStyle: ReturnType<typeof defaultSubtitleStyle>,
  transform?: AudioSubtitleClipTransform,
  sourceTypeForNative: 'video' | 'mic' | 'audio' = 'video',
): SubtitleSegment {
  if (!transform) {
    return {
      id: buildSubtitleId(result.clipId, entry, index),
      startTime: entry.startTime,
      endTime: entry.endTime,
      text: entry.text,
      style: subtitleStyle,
      sourceGroup: {
        kind: sourceTypeForNative === 'mic'
          ? 'mic'
          : sourceTypeForNative === 'audio'
            ? 'audio'
            : 'video',
        assignment: 'generated',
      },
    };
  }

  const sourceLocalStartTime = Math.max(0, entry.startTime - transform.sourceLocalOffsetSec);
  const sourceLocalEndTime = Math.max(sourceLocalStartTime, entry.endTime - transform.sourceLocalOffsetSec);
  return {
    id: buildSubtitleId(result.clipId, {
      ...entry,
      startTime: entry.startTime + transform.timelineOffsetSec,
    }, index),
    startTime: entry.startTime + transform.timelineOffsetSec,
    endTime: entry.endTime + transform.timelineOffsetSec,
    text: entry.text,
    style: subtitleStyle,
    provenance: {
      sourceKind: 'audio',
      audioSegmentId: transform.audioSegmentId,
      sourceName: transform.sourceName,
      sourcePath: transform.sourcePath,
      sourceLocalStartTime,
      sourceLocalEndTime,
    },
    sourceGroup: {
      kind: 'audio',
      assignment: 'generated',
      audioSegmentId: transform.audioSegmentId,
      sourceName: transform.sourceName,
      sourcePath: transform.sourcePath,
    },
  };
}

export function useSubtitleGeneration({
  t,
  projectResetKey,
  segment,
  setSegment,
  composition,
  setComposition,
  activeClipId,
  currentRawVideoPath,
  currentRawMicAudioPath,
  duration,
  setActivePanel,
  persistProject,
}: UseSubtitleGenerationParams) {
  const [editingSubtitleId, setEditingSubtitleId] = useState<string | null>(null);
  const [sourceType, setSourceType] = useState<SubtitleSource>(getInitialSubtitleSource);
  const [subtitleMethod, setSubtitleMethodState] = useState<SubtitleMethod>(getInitialSubtitleMethod);
  const [subtitleMethodNotice, setSubtitleMethodNotice] = useState<string | null>(null);
  const [languageHint, setLanguageHint] = useState(getInitialSubtitleLanguageHint);
  const [geminiPrompt, setGeminiPrompt] = useState(getInitialGeminiPrompt);
  const [groqVocabulary, setGroqVocabulary] = useState<string[]>(getInitialGroqVocabulary);
  const [jobId, setJobId] = useState<string | null>(null);
  const [isStartingSubtitleJob, setIsStartingSubtitleJob] = useState(false);
  const [jobContext, setJobContext] = useState<SubtitleJobContext | null>(null);
  const [status, setStatus] = useState<SubtitleJobViewStatus | null>(null);
  const [capabilities, setCapabilities] = useState<SubtitleGenerationCapabilities | null>(null);
  const lastAppliedResultsKeyRef = useRef('');
  const lastKnownResultsRevisionRef = useRef(0);
  const lastStatusViewKeyRef = useRef('');
  const activeJobIdRef = useRef<string | null>(null);
  const lastProjectResetKeyRef = useRef<string | null | undefined>(undefined);
  const pendingSubtitleResultsRef = useRef<SubtitleClipResult[]>([]);
  const pendingSubtitleApplyTimerRef = useRef<number | null>(null);
  const jobContextRef = useRef<SubtitleJobContext | null>(null);
  const persistProjectRef = useRef<typeof persistProject>(persistProject);
  const pendingCompletedJobPersistRef = useRef(false);
  const lastSubtitleApplyPerfLogRef = useRef(0);
  const partialApplyStateRef = useRef(new Map<string, { signature: string; appliedAt: number }>());

  useEffect(() => {
    jobContextRef.current = jobContext;
  }, [jobContext]);

  useEffect(() => {
    persistProjectRef.current = persistProject;
  }, [persistProject]);

  const clearQueuedSubtitleResults = useCallback(() => {
    if (pendingSubtitleApplyTimerRef.current !== null) {
      window.clearTimeout(pendingSubtitleApplyTimerRef.current);
      pendingSubtitleApplyTimerRef.current = null;
    }
    pendingSubtitleResultsRef.current = [];
  }, []);

  useEffect(() => {
    try {
      localStorage.setItem(SUBTITLE_SOURCE_KEY, sourceType);
    } catch {
      // ignore persistence failures
    }
  }, [sourceType]);

  useEffect(() => {
    try {
      localStorage.setItem(SUBTITLE_METHOD_KEY, subtitleMethod);
    } catch {
      // ignore persistence failures
    }
  }, [subtitleMethod]);

  useEffect(() => {
    try {
      localStorage.setItem(SUBTITLE_LANGUAGE_HINT_KEY, languageHint.trim() || 'auto');
    } catch {
      // ignore persistence failures
    }
  }, [languageHint]);

  useEffect(() => {
    const languageOptions = getSubtitleLanguageOptionsForMethod(subtitleMethod);
    if (!languageOptions.some((option) => option.value === languageHint)) {
      setLanguageHint('auto');
    }
  }, [languageHint, subtitleMethod]);

  useEffect(() => {
    try {
      localStorage.setItem(SUBTITLE_GEMINI_PROMPT_KEY, geminiPrompt);
    } catch {
      // ignore persistence failures
    }
  }, [geminiPrompt]);

  useEffect(() => {
    try {
      localStorage.setItem(SUBTITLE_GROQ_VOCABULARY_KEY, JSON.stringify(groqVocabulary));
    } catch {
      // ignore persistence failures
    }
  }, [groqVocabulary]);

  useEffect(() => {
    activeJobIdRef.current = jobId;
  }, [jobId]);

  useEffect(() => {
    const nextKey = projectResetKey ?? null;
    if (lastProjectResetKeyRef.current === undefined) {
      lastProjectResetKeyRef.current = nextKey;
      return;
    }
    if (lastProjectResetKeyRef.current === nextKey) {
      return;
    }
    lastProjectResetKeyRef.current = nextKey;

    const activeJobId = activeJobIdRef.current;
    if (activeJobId) {
      void invoke('cancel_subtitle_generation', { jobId: activeJobId }).catch(() => {});
    }
    setEditingSubtitleId(null);
    setJobId(null);
    setIsStartingSubtitleJob(false);
    setJobContext(null);
    setStatus(null);
    setSubtitleMethodNotice(null);
    lastAppliedResultsKeyRef.current = '';
    lastKnownResultsRevisionRef.current = 0;
    lastStatusViewKeyRef.current = '';
    clearQueuedSubtitleResults();
  }, [clearQueuedSubtitleResults, projectResetKey]);

  const canUseVideoSource = useMemo(() => {
    if (composition && getEffectiveCompositionMode(composition) === 'unified') {
      return composition.clips.some((clip) => !!clip.rawVideoPath);
    }
    return !!currentRawVideoPath;
  }, [composition, currentRawVideoPath]);

  const canUseMicSource = useMemo(() => {
    if (composition && getEffectiveCompositionMode(composition) === 'unified') {
      return composition.clips.some((clip) => !!clip.rawMicAudioPath);
    }
    return !!currentRawMicAudioPath;
  }, [composition, currentRawMicAudioPath]);

  const canUseAudioSource = useMemo(() => {
    return (composition?.audioSegments?.length ?? 0) > 0;
  }, [composition]);

  useEffect(() => {
    if (sourceType.startsWith('audio:')) {
      const id = sourceType.slice('audio:'.length);
      if (!composition?.audioSegments?.some((segment) => segment.id === id)) {
        setSourceType(composition?.audioSegments?.length ? 'audio' : 'video');
      }
    } else if (sourceType === 'audio' && !canUseAudioSource) {
      setSourceType(canUseVideoSource ? 'video' : canUseMicSource ? 'mic' : 'video');
    }
  }, [canUseMicSource, canUseAudioSource, canUseVideoSource, composition?.audioSegments, sourceType]);

  const refreshSubtitleCapabilities = useCallback(async () => {
    const nextCapabilities = await invoke<SubtitleGenerationCapabilities>(
      'get_subtitle_generation_capabilities',
      {},
    );
    setCapabilities(nextCapabilities);
    return nextCapabilities;
  }, []);

  useEffect(() => {
    void refreshSubtitleCapabilities().catch(() => {
      setCapabilities(null);
    });
  }, [refreshSubtitleCapabilities]);

  const capabilityByMethod = useMemo(
    () => new Map((capabilities?.methods ?? []).map((entry) => [entry.method, entry])),
    [capabilities],
  );

  useEffect(() => {
    const selectedCapability = capabilityByMethod.get(subtitleMethod);
    if (selectedCapability?.available !== false) return;
    const fallbackMethod = capabilities?.methods.find((entry) => entry.available)?.method;
    if (fallbackMethod && fallbackMethod !== subtitleMethod) {
      setSubtitleMethodState(fallbackMethod);
    }
  }, [capabilities, capabilityByMethod, subtitleMethod]);

  const setSubtitleMethod = useCallback(async (nextMethod: SubtitleMethod) => {
    if (nextMethod === subtitleMethod) return;
    setSubtitleMethodNotice(null);
    if (!isQwenLocalSubtitleMethod(nextMethod) && !isParakeetTdtSubtitleMethod(nextMethod)) {
      setSubtitleMethodState(nextMethod);
      return;
    }

    const latestCapabilities = await refreshSubtitleCapabilities().catch(() => capabilities);
    const nextCapability = latestCapabilities?.methods.find((entry) => entry.method === nextMethod);
    if (nextCapability?.available !== false) {
      setSubtitleMethodState(nextMethod);
      return;
    }

    try {
      const result = isParakeetTdtSubtitleMethod(nextMethod)
        ? await invoke<PrepareParakeetTdtResult>('prepare_parakeet_tdt_subtitles', {
            subtitleMethod: nextMethod,
          })
        : await invoke<PrepareQwenLocalResult>('prepare_qwen_local_subtitles', {
            subtitleMethod: nextMethod,
          });
      if (result.available) {
        setSubtitleMethodState(nextMethod);
        return;
      }
      setSubtitleMethodNotice(result.reason ?? nextCapability?.reason ?? null);
    } catch (error) {
      setSubtitleMethodNotice(error instanceof Error ? error.message : String(error));
    }
  }, [capabilities, refreshSubtitleCapabilities, subtitleMethod]);

  const selectedMethodCapability = capabilityByMethod.get(subtitleMethod);
  const canUseSelectedSubtitleMethod = selectedMethodCapability?.available !== false;
  const selectedSubtitleMethodReason = subtitleMethodNotice
    ?? (selectedMethodCapability?.available === false
    ? selectedMethodCapability.reason ?? 'This subtitle method is unavailable.'
    : null);

  const applyResults = useCallback((results: SubtitleClipResult[], context: SubtitleJobContext | null) => {
    if (results.length === 0) return;
    const startedAt = performance.now();
    const subtitleStyle = defaultSubtitleStyle();
    const saveUndoHistory = results.some((result) => !result.isPartial);
    if (!composition || getEffectiveCompositionMode(composition) === 'separate') {
      const applySegmentUpdate = () => setSegment((prev) => {
        if (!prev) return prev;
        return results.reduce((nextSegment, result) => {
          const replacementRanges = context?.replacementRangesByClip[result.clipId] ?? [];
          const transform = context?.clipTransformsByClip[result.clipId];
          const inserted = result.segments.map((entry, index) =>
            buildInsertedSubtitle(
              result,
              entry,
              index,
              subtitleStyle,
              transform,
              context?.sourceTypeForNative,
            ),
          );
          const replacedSegment = transform
            ? replaceAudioSubtitlesOnOriginalTrack(
                nextSegment,
                new Set([transform.audioSegmentId]),
                replacementRanges,
                inserted,
              )
            : result.isPartial
            ? mergePartialOriginalSubtitleSegments(nextSegment, inserted, replacementRanges)
            : replaceOriginalSubtitleSegments(nextSegment, inserted, replacementRanges);
          const updatedSegment = setActiveSubtitleTrackView(
            clearDerivedSubtitleTracks(replacedSegment),
            ORIGINAL_SUBTITLE_TRACK_ID,
          );
          logSubtitleApplyDiagnostics('apply-separate', result, nextSegment, updatedSegment, replacementRanges);
          return updatedSegment;
        }, prev);
      }, saveUndoHistory);
      if (saveUndoHistory) {
        applySegmentUpdate();
      } else {
        startTransition(applySegmentUpdate);
      }
      const elapsedMs = performance.now() - startedAt;
      const now = performance.now();
      if (elapsedMs > 8 && now - lastSubtitleApplyPerfLogRef.current > SUBTITLE_APPLY_PERF_LOG_INTERVAL_MS) {
        lastSubtitleApplyPerfLogRef.current = now;
        const segmentCount = results.reduce((count, result) => count + result.segments.length, 0);
        console.log(
          `[SubtitleGen][Perf] apply mode=separate ms=${elapsedMs.toFixed(1)} results=${results.length} segments=${segmentCount} partial=${results.filter((result) => result.isPartial).length} final=${results.filter((result) => !result.isPartial).length}`,
        );
      }
      return;
    }

    const applyCompositionUpdate = () => setComposition((prev) => {
      if (!prev) return prev;
      const timeline = buildSequenceTimeline(prev);
      if (!timeline) return prev;

      let next = prev;
      for (const result of results) {
        const clip = next.clips.find((entry) => entry.id === result.clipId);
          const transform = context?.clipTransformsByClip[result.clipId];
        const replacementRanges = context?.replacementRangesByClip[result.clipId] ?? [];
        if (transform) {
          const inserted = result.segments.map((entry, index) =>
            buildInsertedSubtitle(
              result,
              entry,
              index,
              subtitleStyle,
              transform,
              context?.sourceTypeForNative,
            ),
          );
          const baseSegment = next.globalSegment ?? segment;
          if (baseSegment) {
            const updatedSegment = setActiveSubtitleTrackView(
              clearDerivedSubtitleTracks(
                replaceAudioSubtitlesOnOriginalTrack(
                  baseSegment,
                  new Set([transform.audioSegmentId]),
                  replacementRanges,
                  inserted,
                ),
              ),
              ORIGINAL_SUBTITLE_TRACK_ID,
            );
            next = {
              ...next,
              globalSegment: updatedSegment,
            };
          }
          continue;
        }
        if (!clip) continue;
        const inserted = result.segments.map((entry, index) =>
          buildInsertedSubtitle(
            result,
            entry,
            index,
            subtitleStyle,
            undefined,
            context?.sourceTypeForNative,
          ),
        );
        const replacedSegment = result.isPartial
          ? mergePartialOriginalSubtitleSegments(
              clip.segment,
              inserted,
              replacementRanges,
            )
          : replaceOriginalSubtitleSegments(
              clip.segment,
              inserted,
              replacementRanges,
            );
        const updatedSegment = setActiveSubtitleTrackView(
          clearDerivedSubtitleTracks(replacedSegment),
          ORIGINAL_SUBTITLE_TRACK_ID,
        );
        logSubtitleApplyDiagnostics('apply-composition', result, clip.segment, updatedSegment, replacementRanges);

        next = updateCompositionClip(next, clip.id, { segment: updatedSegment });

        if (next.globalSegment) {
          const timelineClip = getSequenceClipById(timeline, clip.id);
          if (timelineClip) {
            next = {
              ...next,
              globalSegment: replaceSequenceClipSegmentInGlobal(
                next.globalSegment,
                updatedSegment,
                timelineClip,
                timeline.totalDuration,
              ),
            };
          }
        }
      }
      return next;
    });
    if (saveUndoHistory) {
      applyCompositionUpdate();
    } else {
      startTransition(applyCompositionUpdate);
    }
    const elapsedMs = performance.now() - startedAt;
    const now = performance.now();
    if (elapsedMs > 8 && now - lastSubtitleApplyPerfLogRef.current > SUBTITLE_APPLY_PERF_LOG_INTERVAL_MS) {
      lastSubtitleApplyPerfLogRef.current = now;
      const segmentCount = results.reduce((count, result) => count + result.segments.length, 0);
      console.log(
        `[SubtitleGen][Perf] apply mode=unified ms=${elapsedMs.toFixed(1)} results=${results.length} segments=${segmentCount} partial=${results.filter((result) => result.isPartial).length} final=${results.filter((result) => !result.isPartial).length}`,
      );
    }
  }, [composition, segment, setComposition, setSegment]);

  const applyResultsRef = useRef(applyResults);

  useEffect(() => {
    applyResultsRef.current = applyResults;
  }, [applyResults]);

  const flushQueuedSubtitleResults = useCallback(() => {
    if (pendingSubtitleApplyTimerRef.current !== null) {
      window.clearTimeout(pendingSubtitleApplyTimerRef.current);
      pendingSubtitleApplyTimerRef.current = null;
    }
    const results = pendingSubtitleResultsRef.current;
    pendingSubtitleResultsRef.current = [];
    if (results.length > 0) {
      const segmentCount = results.reduce((count, result) => count + result.segments.length, 0);
      const partialCount = results.filter((result) => result.isPartial).length;
      markFrontendPerfEvent(`subtitle-flush-start results=${results.length} segments=${segmentCount} partial=${partialCount}`);
      applyResultsRef.current(results, jobContextRef.current);
      markFrontendPerfEvent(`subtitle-flush-end results=${results.length} segments=${segmentCount}`);
    }
  }, []);

  const queueSubtitleResults = useCallback((results: SubtitleClipResult[], immediate: boolean) => {
    if (results.length === 0) return;
    const segmentCount = results.reduce((count, result) => count + result.segments.length, 0);
    markFrontendPerfEvent(`subtitle-queue results=${results.length} segments=${segmentCount} immediate=${immediate ? 1 : 0}`);
    if (!immediate && results.every((result) => result.isPartial)) {
      const now = performance.now();
      const shouldApply = results.some((result) => {
        const signature = partialApplySignature(result);
        const previous = partialApplyStateRef.current.get(result.clipId);
        return !previous
          || previous.signature !== signature
          || now - previous.appliedAt >= SUBTITLE_PARTIAL_TEXT_REFRESH_MS;
      });
      if (!shouldApply) {
        markFrontendPerfEvent(`subtitle-partial-skip-stable results=${results.length} segments=${segmentCount}`);
        return;
      }
      for (const result of results) {
        partialApplyStateRef.current.set(result.clipId, {
          signature: partialApplySignature(result),
          appliedAt: now,
        });
      }
    }
    if (immediate) {
      partialApplyStateRef.current.clear();
    }
    pendingSubtitleResultsRef.current = coalesceSubtitleClipResults(
      pendingSubtitleResultsRef.current,
      results,
    );
    if (immediate) {
      flushQueuedSubtitleResults();
      return;
    }
    if (pendingSubtitleApplyTimerRef.current === null) {
      pendingSubtitleApplyTimerRef.current = window.setTimeout(
        flushQueuedSubtitleResults,
        SUBTITLE_PARTIAL_APPLY_INTERVAL_MS,
      );
    }
  }, [flushQueuedSubtitleResults]);

  useEffect(() => {
    if (!composition || getEffectiveCompositionMode(composition) === 'separate') return;
    const effectiveMode = getEffectiveCompositionMode(composition);
    if (effectiveMode !== 'unified') return;
    const timeline = buildSequenceTimeline(composition);
    if (!timeline) return;
    setSegment(mergeCompositionSegmentsToSequence(timeline));
  }, [composition, setSegment]);

  useEffect(() => {
    if (!pendingCompletedJobPersistRef.current || jobId) return;
    if (!segment && !composition) return;

    const timeoutId = window.setTimeout(() => {
      if (!pendingCompletedJobPersistRef.current) return;
      pendingCompletedJobPersistRef.current = false;
      void persistProjectRef.current?.({
        refreshList: true,
        includeMedia: false,
      }).catch((error) => {
        pendingCompletedJobPersistRef.current = true;
        console.warn('[SubtitleGen][Persist] Failed to save completed subtitles:', error);
      });
    }, 50);

    return () => window.clearTimeout(timeoutId);
  }, [composition, jobId, segment]);

  useEffect(() => {
    if (!jobId) return;
    let cancelled = false;

    const poll = async () => {
      try {
        const nextStatus = await invoke<SubtitleJobStatus>(
          'get_subtitle_generation_status',
          {
            jobId,
            knownResultsRevision: lastKnownResultsRevisionRef.current,
          },
        );
        if (cancelled) return;
        const nextViewStatus = stripSubtitleJobResults(nextStatus);
        const nextStatusViewKey = buildSubtitleStatusViewKey(nextViewStatus);
        if (nextStatusViewKey !== lastStatusViewKeyRef.current) {
          lastStatusViewKeyRef.current = nextStatusViewKey;
          setStatus(nextViewStatus);
        }
        const nextAppliedResultsKey = buildAppliedResultsKey(nextStatus.results);
        if (nextStatus.results.length > 0 || nextStatus.state === 'completed') {
          const resultSummary = nextStatus.results
            .map((result) => `${result.clipId}:${result.isPartial ? 'p' : 'f'}:${summarizeSubtitleRanges(result.segments)}`)
            .join(' | ');
          console.log(
            `[SubtitleGen][Diag][poll] state=${nextStatus.state} rev=${nextStatus.resultsRevision} known=${lastKnownResultsRevisionRef.current} `
            + `results=${nextStatus.results.length} ${resultSummary || 'empty-results'}`,
          );
        }
        if (
          nextStatus.results.length > 0
          && nextAppliedResultsKey !== lastAppliedResultsKeyRef.current
        ) {
          if (nextStatus.state === 'completed') {
            clearQueuedSubtitleResults();
          }
          lastKnownResultsRevisionRef.current = nextStatus.resultsRevision;
          lastAppliedResultsKeyRef.current = nextAppliedResultsKey;
          queueSubtitleResults(nextStatus.results, hasFinalSubtitleResult(nextStatus.results));
        }
        if (nextStatus.state === 'completed') {
          flushQueuedSubtitleResults();
          pendingCompletedJobPersistRef.current = true;
          lastAppliedResultsKeyRef.current = '';
          lastKnownResultsRevisionRef.current = 0;
          lastStatusViewKeyRef.current = '';
          setJobId(null);
          setJobContext(null);
          setActivePanel('subtitles');
          return;
        }
        if (nextStatus.state === 'cancelled' || nextStatus.state === 'error') {
          clearQueuedSubtitleResults();
          lastAppliedResultsKeyRef.current = '';
          lastKnownResultsRevisionRef.current = 0;
          lastStatusViewKeyRef.current = '';
          setJobId(null);
          setJobContext(null);
          return;
        }
        window.setTimeout(poll, nextStatus.results.length > 0 ? 120 : 250);
      } catch (error) {
        if (cancelled) return;
        setStatus({
          state: 'error',
          message: error instanceof Error ? error.message : t.subtitleStatusFailed,
          progress: 0,
          activeClipId: null,
          totalClips: 0,
          completedClips: 0,
          resultsRevision: 0,
          skipped: [],
          error: error instanceof Error ? error.message : String(error),
        });
        lastAppliedResultsKeyRef.current = '';
        lastKnownResultsRevisionRef.current = 0;
        lastStatusViewKeyRef.current = '';
        clearQueuedSubtitleResults();
        setJobId(null);
        setJobContext(null);
      }
    };

    void poll();
    return () => {
      cancelled = true;
    };
  }, [
    clearQueuedSubtitleResults,
    flushQueuedSubtitleResults,
    jobId,
    queueSubtitleResults,
    setActivePanel,
    t.subtitleStatusFailed,
  ]);

  const handleGenerateSubtitles = useCallback(async (selectedRange?: TrackSelectionRange | null) => {
    if (jobId || isStartingSubtitleJob) {
      return;
    }
    setIsStartingSubtitleJob(true);
    try {
    const latestCapabilities = await refreshSubtitleCapabilities().catch(() => capabilities);
    const latestSelectedMethod = latestCapabilities?.methods.find((entry) => entry.method === subtitleMethod);
    if (latestSelectedMethod?.available === false) {
      const message = latestSelectedMethod.reason ?? 'Selected subtitle method is unavailable';
      setStatus({
        state: 'error',
        message,
        messageKey: 'subtitleStatusMethodUnavailable',
        progress: 0,
        activeClipId: null,
        totalClips: 0,
        completedClips: 0,
        resultsRevision: 0,
        skipped: [],
        error: message,
      });
      lastStatusViewKeyRef.current = '';
      return;
    }

    const plan = buildSubtitleGenerationPlan({
      segment,
      composition,
      activeClipId,
      currentRawVideoPath,
      currentRawMicAudioPath,
      duration,
      sourceType,
      selectedRange,
    });

    if (plan.clips.length === 0) {
      const message = selectedRange
        ? t.subtitleStatusNoSourceForRange
        : t.subtitleStatusNoSource;
      setStatus({
        state: 'error',
        message,
        progress: 0,
        activeClipId: null,
        totalClips: 0,
        completedClips: 0,
        resultsRevision: 0,
        skipped: [],
        error: message,
      });
      lastStatusViewKeyRef.current = '';
      return;
    }

    setActivePanel('subtitles');
    clearQueuedSubtitleResults();
    setJobContext({
      replacementRangesByClip: plan.replacementRangesByClip,
      indicator: plan.indicator,
      sourceTypeForNative: plan.sourceTypeForNative,
      clipTransformsByClip: plan.clipTransformsByClip,
    });

    const result = await invoke<{ jobId: string }>('start_subtitle_generation', {
      sourceType: plan.sourceTypeForNative,
      subtitleMethod,
      languageHint: languageHint.trim() || 'auto',
      geminiPrompt: geminiPrompt.trim() || null,
      groqVocabulary,
      clips: plan.clips,
    });

    setStatus({
      state: 'queued',
      message: t.subtitleStatusStarting,
      messageKey: 'subtitleStatusStarting',
      progress: 0,
      activeClipId: null,
      totalClips: plan.clips.length,
      completedClips: 0,
      resultsRevision: 0,
      skipped: [],
      error: null,
    });
    lastAppliedResultsKeyRef.current = '';
    lastKnownResultsRevisionRef.current = 0;
    lastStatusViewKeyRef.current = '';
    setJobId(result.jobId);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setStatus({
        state: 'error',
        message,
        progress: 0,
        activeClipId: null,
        totalClips: 0,
        completedClips: 0,
        resultsRevision: 0,
        skipped: [],
        error: message,
      });
      lastStatusViewKeyRef.current = '';
    } finally {
      setIsStartingSubtitleJob(false);
    }
  }, [
    activeClipId,
    capabilities,
    clearQueuedSubtitleResults,
    composition,
    currentRawMicAudioPath,
    currentRawVideoPath,
    duration,
    isStartingSubtitleJob,
    jobId,
    geminiPrompt,
    groqVocabulary,
    languageHint,
    refreshSubtitleCapabilities,
    segment,
    setActivePanel,
    sourceType,
    subtitleMethod,
  ]);

  const handleCancelSubtitleGeneration = useCallback(async () => {
    if (!jobId) return;
    await invoke('cancel_subtitle_generation', { jobId });
    clearQueuedSubtitleResults();
    setStatus((prev) => (prev ? {
      ...prev,
      state: 'cancelled',
      message: t.subtitleStatusCancelled,
      messageKey: 'subtitleStatusCancelled',
      messageParams: {},
    } : prev));
    lastAppliedResultsKeyRef.current = '';
    lastKnownResultsRevisionRef.current = 0;
    lastStatusViewKeyRef.current = '';
    setJobId(null);
    setJobContext(null);
  }, [clearQueuedSubtitleResults, jobId, t.subtitleStatusCancelled]);

  useEffect(() => clearQueuedSubtitleResults, [clearQueuedSubtitleResults]);

  return {
    editingSubtitleId,
    setEditingSubtitleId,
    subtitleSource: sourceType,
    setSubtitleSource: setSourceType,
    subtitleMethod,
    setSubtitleMethod,
    subtitleMethodCapabilities: capabilities?.methods ?? DEFAULT_SUBTITLE_METHOD_CAPABILITIES,
    canUseSelectedSubtitleMethod,
    selectedSubtitleMethodReason,
    subtitleLanguageHint: languageHint,
    setSubtitleLanguageHint: setLanguageHint,
    subtitleGeminiPrompt: geminiPrompt,
    setSubtitleGeminiPrompt: setGeminiPrompt,
    subtitleGroqVocabulary: groqVocabulary,
    setSubtitleGroqVocabulary: setGroqVocabulary,
    isGeneratingSubtitles: !!jobId || isStartingSubtitleJob,
    subtitleStatusMessage: localizeSubtitleStatus(t, status),
    subtitleActiveClipId: status?.activeClipId ?? null,
    subtitleGenerationIndicator: jobContext?.indicator ?? null,
    canUseVideoSubtitleSource: canUseVideoSource,
    canUseMicSubtitleSource: canUseMicSource,
    canUseAudioSubtitleSource: canUseAudioSource,
    handleGenerateSubtitles,
    handleCancelSubtitleGeneration,
  };
}
