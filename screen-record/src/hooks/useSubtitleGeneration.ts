import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
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
  type SubtitleGenerationIndicator,
} from '@/lib/subtitleGenerationPlan';
import { defaultSubtitleStyle } from '@/lib/subtitleDefaults';
import {
  type TrackSelectionRange,
} from '@/lib/timelineSegmentSelection';
import type { Translations } from '@/i18n';
import type { ProjectComposition, VideoSegment } from '@/types/video';
import {
  mergePartialOriginalSubtitleSegments,
  replaceOriginalSubtitleSegments,
} from '@/lib/subtitleTrackMutations';

export type SubtitleMethod =
  | 'groq-whisper-accurate'
  | 'groq-whisper-large-v3-turbo'
  | 'qwen-local-0-6b'
  | 'qwen-local-1-7b';

const DEFAULT_SUBTITLE_METHOD_CAPABILITIES: Array<{
  method: SubtitleMethod;
  available: boolean;
  reason?: string | null;
}> = [
  { method: 'groq-whisper-accurate', available: true, reason: null },
  { method: 'groq-whisper-large-v3-turbo', available: true, reason: null },
  { method: 'qwen-local-0-6b', available: false, reason: null },
  { method: 'qwen-local-1-7b', available: false, reason: null },
];

const SUBTITLE_SOURCE_KEY = 'screen-record-subtitle-source-v1';
const SUBTITLE_METHOD_KEY = 'screen-record-subtitle-method-v1';
const SUBTITLE_LANGUAGE_HINT_KEY = 'screen-record-subtitle-language-hint-v1';

function getInitialSubtitleSource(): 'video' | 'mic' {
  try {
    const raw = localStorage.getItem(SUBTITLE_SOURCE_KEY);
    if (raw === 'video' || raw === 'mic') {
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
    value === 'qwen-local-0-6b' ||
    value === 'qwen-local-1-7b'
  );
}

function normalizeStoredSubtitleMethod(value: string | null): SubtitleMethod | null {
  if (value === 'gemini-live-3-1-flash-preview') {
    return 'groq-whisper-accurate';
  }
  return value && isSubtitleMethod(value) ? value : null;
}

function isQwenLocalSubtitleMethod(method: SubtitleMethod) {
  return method === 'qwen-local-0-6b' || method === 'qwen-local-1-7b';
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
}

interface UseSubtitleGenerationParams {
  t: Translations;
  segment: VideoSegment | null;
  setSegment: (
    segment:
      | VideoSegment
      | null
      | ((prev: VideoSegment | null) => VideoSegment | null),
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
    status.resultsRevision,
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

function formatResultSummary(results: SubtitleClipResult[]) {
  if (results.length === 0) return 'none';
  return results.map((result) => {
    const tail = result.segments[result.segments.length - 1];
    const tailSummary = tail
      ? `${tail.startTime.toFixed(2)}-${tail.endTime.toFixed(2)}:${tail.text.slice(0, 24).replace(/\s+/g, ' ')}`
      : 'none';
    return `${result.clipId}:${result.isPartial ? 'partial' : 'final'}:${result.segments.length}:${tailSummary}`;
  }).join(' | ');
}

export function useSubtitleGeneration({
  t,
  segment,
  setSegment,
  composition,
  setComposition,
  activeClipId,
  currentRawVideoPath,
  currentRawMicAudioPath,
  duration,
  setActivePanel,
}: UseSubtitleGenerationParams) {
  const [editingSubtitleId, setEditingSubtitleId] = useState<string | null>(null);
  const [sourceType, setSourceType] = useState<'video' | 'mic'>(getInitialSubtitleSource);
  const [subtitleMethod, setSubtitleMethodState] = useState<SubtitleMethod>(getInitialSubtitleMethod);
  const [subtitleMethodNotice, setSubtitleMethodNotice] = useState<string | null>(null);
  const [languageHint, setLanguageHint] = useState(getInitialSubtitleLanguageHint);
  const [jobId, setJobId] = useState<string | null>(null);
  const [isStartingSubtitleJob, setIsStartingSubtitleJob] = useState(false);
  const [jobContext, setJobContext] = useState<SubtitleJobContext | null>(null);
  const [status, setStatus] = useState<SubtitleJobViewStatus | null>(null);
  const [capabilities, setCapabilities] = useState<SubtitleGenerationCapabilities | null>(null);
  const lastAppliedResultsKeyRef = useRef('');
  const lastKnownResultsRevisionRef = useRef(0);
  const lastStatusViewKeyRef = useRef('');

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
    if (!isQwenLocalSubtitleMethod(nextMethod)) {
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
      const result = await invoke<PrepareQwenLocalResult>('prepare_qwen_local_subtitles', {
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
    const mode = !composition || getEffectiveCompositionMode(composition) === 'separate' ? 'separate' : 'unified';
    console.log(
      `[SubtitleGen][Webview][apply-results] start mode=${mode} results=${results.length} partial=${results.filter((result) => result.isPartial).length} final=${results.filter((result) => !result.isPartial).length} clips=${formatResultSummary(results)}`,
    );
    const subtitleStyle = defaultSubtitleStyle();

    if (!composition || getEffectiveCompositionMode(composition) === 'separate') {
      const rootResult = results[0];
      if (!rootResult) return;
      const replacementRanges = context?.replacementRangesByClip[rootResult.clipId] ?? [];
      setSegment((prev) => {
        if (!prev) return prev;
        const inserted = rootResult.segments.map((entry, index) => ({
          id: buildSubtitleId(rootResult.clipId, entry, index),
          startTime: entry.startTime,
          endTime: entry.endTime,
          text: entry.text,
          style: subtitleStyle,
        }));
        return rootResult.isPartial
          ? mergePartialOriginalSubtitleSegments(prev, inserted, replacementRanges)
          : replaceOriginalSubtitleSegments(prev, inserted, replacementRanges);
      });
      console.log(
        `[SubtitleGen][Webview][apply-results] queued mode=separate elapsedMs=${(performance.now() - startedAt).toFixed(2)} clips=${formatResultSummary(results)}`,
      );
      return;
    }

    setComposition((prev) => {
      if (!prev) return prev;
      const timeline = buildSequenceTimeline(prev);
      if (!timeline) return prev;

      let next = prev;
      for (const result of results) {
        const clip = next.clips.find((entry) => entry.id === result.clipId);
        if (!clip) continue;
        const replacementRanges = context?.replacementRangesByClip[result.clipId] ?? [];
        const inserted = result.segments.map((entry, index) => ({
          id: buildSubtitleId(result.clipId, entry, index),
          startTime: entry.startTime,
          endTime: entry.endTime,
          text: entry.text,
          style: subtitleStyle,
        }));
        const updatedSegment = result.isPartial
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
    console.log(
      `[SubtitleGen][Webview][apply-results] queued mode=unified elapsedMs=${(performance.now() - startedAt).toFixed(2)} clips=${formatResultSummary(results)}`,
    );
  }, [composition, setComposition, setSegment]);

  useEffect(() => {
    if (!composition || getEffectiveCompositionMode(composition) === 'separate') return;
    const effectiveMode = getEffectiveCompositionMode(composition);
    if (effectiveMode !== 'unified') return;
    const timeline = buildSequenceTimeline(composition);
    if (!timeline) return;
    setSegment(mergeCompositionSegmentsToSequence(timeline));
  }, [composition, setSegment]);

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
        if (
          nextStatus.results.length > 0
          && nextAppliedResultsKey !== lastAppliedResultsKeyRef.current
        ) {
          console.log(
            `[SubtitleGen][Webview][poll-results] knownRev=${lastKnownResultsRevisionRef.current} nextRev=${nextStatus.resultsRevision} results=${nextStatus.results.length} clips=${formatResultSummary(nextStatus.results)}`,
          );
          lastKnownResultsRevisionRef.current = nextStatus.resultsRevision;
          lastAppliedResultsKeyRef.current = nextAppliedResultsKey;
          applyResults(nextStatus.results, jobContext);
        }
        if (nextStatus.state === 'completed') {
          applyResults(nextStatus.results, jobContext);
          lastAppliedResultsKeyRef.current = '';
          lastKnownResultsRevisionRef.current = 0;
          lastStatusViewKeyRef.current = '';
          setJobId(null);
          setJobContext(null);
          setActivePanel('subtitles');
          return;
        }
        if (nextStatus.state === 'cancelled' || nextStatus.state === 'error') {
          lastAppliedResultsKeyRef.current = '';
          lastKnownResultsRevisionRef.current = 0;
          lastStatusViewKeyRef.current = '';
          setJobId(null);
          setJobContext(null);
          return;
        }
        window.setTimeout(poll, 400);
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
        setJobId(null);
        setJobContext(null);
      }
    };

    void poll();
    return () => {
      cancelled = true;
    };
  }, [applyResults, jobContext, jobId, setActivePanel]);

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
    setJobContext({
      replacementRangesByClip: plan.replacementRangesByClip,
      indicator: plan.indicator,
    });

    const result = await invoke<{ jobId: string }>('start_subtitle_generation', {
      sourceType,
      subtitleMethod,
      languageHint: languageHint.trim() || 'auto',
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
    composition,
    currentRawMicAudioPath,
    currentRawVideoPath,
    duration,
    isStartingSubtitleJob,
    jobId,
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
  }, [jobId]);

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
    isGeneratingSubtitles: !!jobId || isStartingSubtitleJob,
    subtitleStatusMessage: localizeSubtitleStatus(t, status),
    subtitleActiveClipId: status?.activeClipId ?? null,
    subtitleGenerationIndicator: jobContext?.indicator ?? null,
    canUseVideoSubtitleSource: canUseVideoSource,
    canUseMicSubtitleSource: canUseMicSource,
    handleGenerateSubtitles,
    handleCancelSubtitleGeneration,
  };
}
