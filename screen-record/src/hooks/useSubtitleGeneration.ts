import { useCallback, useEffect, useMemo, useState } from 'react';
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
  replaceSegmentsInRanges,
  type TrackSelectionRange,
} from '@/lib/timelineSegmentSelection';
import type { Translations } from '@/i18n';
import type { ProjectComposition, VideoSegment } from '@/types/video';

export type SubtitleMethod =
  | 'groq-whisper-accurate'
  | 'groq-whisper-large-v3-turbo'
  | 'gemini-live-3-1-flash-preview'
  | 'qwen-local-0-6b'
  | 'qwen-local-1-7b';

const DEFAULT_SUBTITLE_METHOD_CAPABILITIES: Array<{
  method: SubtitleMethod;
  available: boolean;
  reason?: string | null;
}> = [
  { method: 'groq-whisper-accurate', available: true, reason: null },
  { method: 'groq-whisper-large-v3-turbo', available: true, reason: null },
  { method: 'gemini-live-3-1-flash-preview', available: false, reason: null },
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
    value === 'gemini-live-3-1-flash-preview' ||
    value === 'qwen-local-0-6b' ||
    value === 'qwen-local-1-7b'
  );
}

function getInitialSubtitleMethod(): SubtitleMethod {
  try {
    const raw = localStorage.getItem(SUBTITLE_METHOD_KEY);
    if (raw && isSubtitleMethod(raw)) {
      return raw;
    }
  } catch {
    // ignore persistence failures
  }
  return 'gemini-live-3-1-flash-preview';
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

interface SubtitleJobStatus {
  state: 'queued' | 'running' | 'completed' | 'cancelled' | 'error';
  message: string;
  messageKey?: string | null;
  messageParams?: Record<string, string> | null;
  progress: number;
  activeClipId?: string | null;
  totalClips: number;
  completedClips: number;
  results: SubtitleClipResult[];
  skipped: SubtitleSkippedClip[];
  error?: string | null;
}

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
  index: number,
) {
  return `subtitle-${clipId}-${Math.round(entry.startTime * 1000)}-${Math.round(entry.endTime * 1000)}-${index}`;
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
  status: Pick<SubtitleJobStatus, 'message' | 'messageKey' | 'messageParams'> | null,
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
  const [subtitleMethod, setSubtitleMethod] = useState<SubtitleMethod>(getInitialSubtitleMethod);
  const [languageHint, setLanguageHint] = useState(getInitialSubtitleLanguageHint);
  const [jobId, setJobId] = useState<string | null>(null);
  const [jobContext, setJobContext] = useState<SubtitleJobContext | null>(null);
  const [status, setStatus] = useState<SubtitleJobStatus | null>(null);
  const [capabilities, setCapabilities] = useState<SubtitleGenerationCapabilities | null>(null);

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
      setSubtitleMethod(fallbackMethod);
    }
  }, [capabilities, capabilityByMethod, subtitleMethod]);

  const selectedMethodCapability = capabilityByMethod.get(subtitleMethod);
  const canUseSelectedSubtitleMethod = selectedMethodCapability?.available !== false;
  const selectedSubtitleMethodReason = selectedMethodCapability?.available === false
    ? selectedMethodCapability.reason ?? 'This subtitle method is unavailable.'
    : null;

  const applyResults = useCallback((results: SubtitleClipResult[], context: SubtitleJobContext | null) => {
    if (results.length === 0) return;
    const subtitleStyle = defaultSubtitleStyle();
    const isRangeJob = context?.indicator.mode === 'range';

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
        const subtitleSegments =
          isRangeJob && replacementRanges.length > 0
            ? replaceSegmentsInRanges(prev.subtitleSegments ?? [], replacementRanges, inserted)
            : inserted;
        return {
          ...prev,
          subtitleSegments,
        };
      });
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
        const updatedSegment = {
          ...clip.segment,
          subtitleSegments:
            isRangeJob && replacementRanges.length > 0
              ? replaceSegmentsInRanges(
                  clip.segment.subtitleSegments ?? [],
                  replacementRanges,
                  inserted,
                )
              : inserted,
        };

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
          { jobId },
        );
        if (cancelled) return;
        setStatus(nextStatus);
        if (nextStatus.results.length > 0) {
          applyResults(nextStatus.results, jobContext);
        }
        if (nextStatus.state === 'completed') {
          applyResults(nextStatus.results, jobContext);
          setJobId(null);
          setJobContext(null);
          setActivePanel('subtitles');
          return;
        }
        if (nextStatus.state === 'cancelled' || nextStatus.state === 'error') {
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
          results: [],
          skipped: [],
          error: error instanceof Error ? error.message : String(error),
        });
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
        results: [],
        skipped: [],
        error: message,
      });
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
        results: [],
        skipped: [],
        error: message,
      });
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
      results: [],
      skipped: [],
      error: null,
    });
    setJobId(result.jobId);
  }, [
    activeClipId,
    capabilities,
    composition,
    currentRawMicAudioPath,
    currentRawVideoPath,
    duration,
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
    isGeneratingSubtitles: !!jobId,
    subtitleStatusMessage: localizeSubtitleStatus(t, status),
    subtitleActiveClipId: status?.activeClipId ?? null,
    subtitleGenerationIndicator: jobContext?.indicator ?? null,
    canUseVideoSubtitleSource: canUseVideoSource,
    canUseMicSubtitleSource: canUseMicSource,
    handleGenerateSubtitles,
    handleCancelSubtitleGeneration,
  };
}
