import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@/lib/ipc';
import { getEffectiveCompositionMode } from '@/lib/projectComposition';
import {
  buildSequenceTimeline,
  mergeCompositionSegmentsToSequence,
} from '@/lib/sequenceTimeline';
import {
  buildSubtitleGenerationPlan,
  type SubtitleSource,
} from '@/lib/subtitleGenerationPlan';
import {
  type TrackSelectionRange,
} from '@/lib/timelineSegmentSelection';
import { getSubtitleLanguageOptionsForMethod } from '@/lib/subtitleLanguageOptions';
import {
  buildAppliedResultsKey,
  buildSubtitleStatusViewKey,
  hasFinalSubtitleResult,
  localizeSubtitleStatus,
  splitGeneratedSubtitleResults,
  stripSubtitleJobResults,
  summarizeSubtitleRanges,
} from './subtitleGenerationResults';
import {
  DEFAULT_SUBTITLE_METHOD_CAPABILITIES,
  getInitialAutoSplitEnabled,
  getInitialAutoSplitMaxUnits,
  getInitialGeminiPrompt,
  getInitialGroqVocabulary,
  getInitialSubtitleLanguageHint,
  getInitialSubtitleMethod,
  getInitialSubtitleSource,
  isParakeetTdtSubtitleMethod,
  isQwenLocalSubtitleMethod,
  persistAutoSplitEnabled,
  persistAutoSplitMaxUnits,
  persistGeminiPrompt,
  persistGroqVocabulary,
  persistSubtitleLanguageHint,
  persistSubtitleMethod,
  persistSubtitleSource,
} from './subtitleGenerationStorage';
import type {
  PrepareParakeetTdtResult,
  PrepareQwenLocalResult,
  SubtitleGenerationCapabilities,
  SubtitleJobContext,
  SubtitleJobStatus,
  SubtitleJobViewStatus,
  SubtitleMethod,
  UseSubtitleGenerationParams,
} from './subtitleGenerationTypes';
import { useSubtitleResultApplication } from './useSubtitleResultApplication';
import { useAsyncJobPoll, buildCancelHandler } from './useAsyncJobPoll';
import {
  clearResumableRun,
  saveResumableRun,
  useResumableRun,
} from './resumableJobRegistry';

export type { SubtitleMethod } from './subtitleGenerationTypes';

interface ResumableSubtitleGenerationRun {
  jobId: string;
  jobContext: SubtitleJobContext | null;
  status: SubtitleJobViewStatus | null;
  lastKnownResultsRevision: number;
  lastAppliedResultsKey: string;
  lastStatusViewKey: string;
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
  const [autoSplitSubtitles, setAutoSplitSubtitles] = useState(getInitialAutoSplitEnabled);
  const [autoSplitMaxUnits, setAutoSplitMaxUnitsState] = useState(getInitialAutoSplitMaxUnits);
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
  const autoSplitSubtitlesRef = useRef(autoSplitSubtitles);
  const autoSplitMaxUnitsRef = useRef(autoSplitMaxUnits);
  const runNamespace = 'subtitle-generation';

  // Re-adopt an in-flight subtitle job after the panel remounts (tab switch).
  useResumableRun<ResumableSubtitleGenerationRun>(runNamespace, jobId, (cached) => {
    lastKnownResultsRevisionRef.current = cached.lastKnownResultsRevision;
    lastAppliedResultsKeyRef.current = cached.lastAppliedResultsKey;
    lastStatusViewKeyRef.current = cached.lastStatusViewKey;
    setJobContext(cached.jobContext);
    setStatus(cached.status);
    setJobId(cached.jobId);
  });

  const {
    clearQueuedSubtitleResults,
    flushQueuedSubtitleResults,
    markCompletedJobForPersist,
    queueSubtitleResults,
  } = useSubtitleResultApplication({
    segment,
    setSegment,
    composition,
    setComposition,
    jobContext,
    jobId,
    persistProject,
  });

  useEffect(() => {
    persistSubtitleSource(sourceType);
  }, [sourceType]);

  useEffect(() => {
    persistSubtitleMethod(subtitleMethod);
  }, [subtitleMethod]);

  useEffect(() => {
    persistSubtitleLanguageHint(languageHint);
  }, [languageHint]);

  useEffect(() => {
    const languageOptions = getSubtitleLanguageOptionsForMethod(subtitleMethod);
    if (!languageOptions.some((option) => option.value === languageHint)) {
      setLanguageHint('auto');
    }
  }, [languageHint, subtitleMethod]);

  useEffect(() => {
    persistGeminiPrompt(geminiPrompt);
  }, [geminiPrompt]);

  useEffect(() => {
    persistGroqVocabulary(groqVocabulary);
  }, [groqVocabulary]);

  useEffect(() => {
    autoSplitSubtitlesRef.current = autoSplitSubtitles;
    persistAutoSplitEnabled(autoSplitSubtitles);
  }, [autoSplitSubtitles]);

  const setAutoSplitMaxUnits = useCallback((value: number) => {
    const next = Math.min(24, Math.max(3, Math.round(value)));
    setAutoSplitMaxUnitsState(next);
  }, []);

  useEffect(() => {
    autoSplitMaxUnitsRef.current = autoSplitMaxUnits;
    persistAutoSplitMaxUnits(autoSplitMaxUnits);
  }, [autoSplitMaxUnits]);

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
    clearResumableRun(runNamespace);
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

  const updateMethodCapability = useCallback((
    method: SubtitleMethod,
    available: boolean,
    reason?: string | null,
  ) => {
    setCapabilities((prev) => {
      const currentMethods = prev?.methods ?? DEFAULT_SUBTITLE_METHOD_CAPABILITIES;
      return {
        methods: currentMethods.map((entry) => (
          entry.method === method
            ? { ...entry, available, reason: reason ?? null }
            : entry
        )),
      };
    });
  }, []);

  const capabilityByMethod = useMemo(
    () => new Map((capabilities?.methods ?? []).map((entry) => [entry.method, entry])),
    [capabilities],
  );

  const prepareLocalSubtitleMethod = useCallback(async (method: SubtitleMethod) => {
    if (!isQwenLocalSubtitleMethod(method) && !isParakeetTdtSubtitleMethod(method)) {
      updateMethodCapability(method, true, null);
      return { available: true, startedDownloads: false, reason: null };
    }

    const result = isParakeetTdtSubtitleMethod(method)
      ? await invoke<PrepareParakeetTdtResult>('prepare_parakeet_tdt_subtitles', {
          subtitleMethod: method,
        })
      : await invoke<PrepareQwenLocalResult>('prepare_qwen_local_subtitles', {
          subtitleMethod: method,
        });
    updateMethodCapability(method, result.available, result.reason ?? null);
    return result;
  }, [updateMethodCapability]);

  const setSubtitleMethod = useCallback(async (nextMethod: SubtitleMethod) => {
    if (nextMethod === subtitleMethod) return;
    setSubtitleMethodNotice(null);
    if (!isQwenLocalSubtitleMethod(nextMethod) && !isParakeetTdtSubtitleMethod(nextMethod)) {
      updateMethodCapability(nextMethod, true, null);
      setSubtitleMethodState(nextMethod);
      return;
    }

    try {
      const result = await prepareLocalSubtitleMethod(nextMethod);
      if (result.available) {
        setSubtitleMethodState(nextMethod);
        return;
      }
      setSubtitleMethodNotice(result.reason ?? null);
    } catch (error) {
      setSubtitleMethodNotice(error instanceof Error ? error.message : String(error));
    }
  }, [prepareLocalSubtitleMethod, subtitleMethod, updateMethodCapability]);

  const selectedMethodCapability = capabilityByMethod.get(subtitleMethod);
  const selectedMethodIsLocal = isQwenLocalSubtitleMethod(subtitleMethod)
    || isParakeetTdtSubtitleMethod(subtitleMethod);
  const canUseSelectedSubtitleMethod = selectedMethodCapability?.available !== false
    || selectedMethodIsLocal;
  const selectedSubtitleMethodReason = subtitleMethodNotice
    ?? (selectedMethodCapability?.available === false
    ? selectedMethodCapability.reason ?? 'This subtitle method is unavailable.'
    : null);

  useEffect(() => {
    if (!composition || getEffectiveCompositionMode(composition) === 'separate') return;
    const effectiveMode = getEffectiveCompositionMode(composition);
    if (effectiveMode !== 'unified') return;
    const timeline = buildSequenceTimeline(composition);
    if (!timeline) return;
    setSegment(mergeCompositionSegmentsToSequence(timeline));
  }, [composition, setSegment]);

  useAsyncJobPoll<SubtitleJobStatus>({
    jobId,
    fetchStatus: (activeJobId) =>
      invoke<SubtitleJobStatus>('get_subtitle_generation_status', {
        jobId: activeJobId,
        knownResultsRevision: lastKnownResultsRevisionRef.current,
      }),
    isTerminal: (nextStatus) =>
      nextStatus.state === 'completed'
      || nextStatus.state === 'cancelled'
      || nextStatus.state === 'error',
    onTick: (nextStatus) => {
      const nextViewStatus = stripSubtitleJobResults(nextStatus);
      const nextStatusViewKey = buildSubtitleStatusViewKey(nextViewStatus);
      if (nextStatusViewKey !== lastStatusViewKeyRef.current) {
        lastStatusViewKeyRef.current = nextStatusViewKey;
        setStatus(nextViewStatus);
      }
      const generatedResults = splitGeneratedSubtitleResults(
        nextStatus.results,
        autoSplitSubtitlesRef.current,
        autoSplitMaxUnitsRef.current,
      );
      const nextAppliedResultsKey = buildAppliedResultsKey(generatedResults);
      if (nextStatus.results.length > 0 || nextStatus.state === 'completed') {
        const resultSummary = generatedResults
          .map((result) => `${result.clipId}:${result.isPartial ? 'p' : 'f'}:${summarizeSubtitleRanges(result.segments)}`)
          .join(' | ');
        console.log(
          `[SubtitleGen][Diag][poll] state=${nextStatus.state} rev=${nextStatus.resultsRevision} known=${lastKnownResultsRevisionRef.current} `
          + `results=${nextStatus.results.length} ${resultSummary || 'empty-results'}`,
        );
      }
      if (
        generatedResults.length > 0
        && nextAppliedResultsKey !== lastAppliedResultsKeyRef.current
      ) {
        if (nextStatus.state === 'completed') {
          clearQueuedSubtitleResults();
        }
        lastKnownResultsRevisionRef.current = nextStatus.resultsRevision;
        lastAppliedResultsKeyRef.current = nextAppliedResultsKey;
        queueSubtitleResults(generatedResults, hasFinalSubtitleResult(generatedResults));
      }
      if (jobId) {
        saveResumableRun<ResumableSubtitleGenerationRun>(runNamespace, {
          jobId,
          jobContext,
          status: nextViewStatus,
          lastKnownResultsRevision: lastKnownResultsRevisionRef.current,
          lastAppliedResultsKey: lastAppliedResultsKeyRef.current,
          lastStatusViewKey: lastStatusViewKeyRef.current,
        });
      }
    },
    onComplete: (nextStatus) => {
      clearResumableRun(runNamespace);
      if (nextStatus.state === 'completed') {
        flushQueuedSubtitleResults();
        markCompletedJobForPersist();
        lastAppliedResultsKeyRef.current = '';
        lastKnownResultsRevisionRef.current = 0;
        lastStatusViewKeyRef.current = '';
        setJobId(null);
        setJobContext(null);
        setActivePanel('subtitles');
        return;
      }
      // cancelled or error terminal state
      clearQueuedSubtitleResults();
      lastAppliedResultsKeyRef.current = '';
      lastKnownResultsRevisionRef.current = 0;
      lastStatusViewKeyRef.current = '';
      setJobId(null);
      setJobContext(null);
    },
    onError: (error) => {
      clearResumableRun(runNamespace);
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
    },
    intervalFor: (nextStatus) => (nextStatus.results.length > 0 ? 120 : 250),
  });

  const handleGenerateSubtitles = useCallback(async (selectedRange?: TrackSelectionRange | null) => {
    if (jobId || isStartingSubtitleJob) {
      return;
    }
    setIsStartingSubtitleJob(true);
    try {
    if (selectedMethodIsLocal) {
      const preparation = await prepareLocalSubtitleMethod(subtitleMethod);
      if (!preparation.available) {
        const message = preparation.reason ?? 'Selected subtitle method is unavailable';
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
        setSubtitleMethodNotice(message);
        lastStatusViewKeyRef.current = '';
        return;
      }
      setSubtitleMethodNotice(null);
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
    const nextJobContext: SubtitleJobContext = {
      replacementRangesByClip: plan.replacementRangesByClip,
      indicator: plan.indicator,
      sourceTypeForNative: plan.sourceTypeForNative,
      clipTransformsByClip: plan.clipTransformsByClip,
    };
    setJobContext(nextJobContext);

    const result = await invoke<{ jobId: string }>('start_subtitle_generation', {
      sourceType: plan.sourceTypeForNative,
      subtitleMethod,
      languageHint: languageHint.trim() || 'auto',
      geminiPrompt: geminiPrompt.trim() || null,
      groqVocabulary,
      clips: plan.clips,
    });

    const queuedStatus: SubtitleJobViewStatus = {
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
    };
    setStatus(queuedStatus);
    lastAppliedResultsKeyRef.current = '';
    lastKnownResultsRevisionRef.current = 0;
    lastStatusViewKeyRef.current = '';
    setJobId(result.jobId);
    saveResumableRun<ResumableSubtitleGenerationRun>(runNamespace, {
      jobId: result.jobId,
      jobContext: nextJobContext,
      status: queuedStatus,
      lastKnownResultsRevision: 0,
      lastAppliedResultsKey: '',
      lastStatusViewKey: '',
    });
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
    prepareLocalSubtitleMethod,
    segment,
    selectedMethodIsLocal,
    setActivePanel,
    sourceType,
    subtitleMethod,
  ]);

  const handleCancelSubtitleGeneration = useCallback(
    buildCancelHandler({
      jobId,
      cancelCommand: 'cancel_subtitle_generation',
      onCancelled: () => {
        clearResumableRun(runNamespace);
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
      },
    }),
    [clearQueuedSubtitleResults, jobId, t.subtitleStatusCancelled],
  );

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
    autoSplitSubtitles,
    setAutoSplitSubtitles,
    autoSplitMaxUnits,
    setAutoSplitMaxUnits,
  };
}
