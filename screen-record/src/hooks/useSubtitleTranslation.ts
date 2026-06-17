import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@/lib/ipc';
import { useAsyncJobPoll, buildCancelHandler } from './useAsyncJobPoll';
import {
  clearResumableRun,
  saveResumableRun,
  useResumableRun,
} from './resumableJobRegistry';
import {
  getEffectiveCompositionMode,
  updateCompositionClip,
} from '@/lib/projectComposition';
import {
  buildSequenceTimeline,
  getSequenceClipById,
  replaceSequenceClipSegmentInGlobal,
} from '@/lib/sequenceTimeline';
import { SUBTITLE_LANGUAGE_OPTIONS } from '@/lib/subtitleLanguageOptions';
import {
  getActiveSubtitleView,
  findTranslationTrackByLanguage,
  getSubtitleTrackLabel,
  getSubtitleTracks,
  getVisibleSubtitleSegments,
  ORIGINAL_SUBTITLE_TRACK_ID,
  removeSubtitleTrack,
  setActiveSubtitleCustomView,
  setActiveSubtitleTrackView,
  setSubtitleCustomChain,
} from '@/lib/subtitleTracks';
import type { Translations } from '@/i18n';
import type {
  ProjectComposition,
  SubtitleChainItem,
  VideoSegment,
} from '@/types/video';
import {
  applyTranslationResultsToSegment,
  buildCompositionTranslationItems,
  buildTranslationChunkPreview,
  buildTranslationItems,
  clampTranslationChunkCount,
  getInitialSmartFallback,
  getInitialTranslationChunkCount,
  getInitialTranslationInstructions,
  getInitialTranslationLanguage,
  getInitialTranslationModelId,
  getInitialTranslationSource,
  GTX_TRANSLATION_MODEL_ID,
  localizeStatus,
  persistTranslationChunkCount,
  persistTranslationInstructions,
  persistTranslationLanguage,
  persistTranslationModelId,
  persistTranslationSmartFallback,
  persistTranslationSource,
  suggestTranslationChunkCount,
  updateSubtitleViewAcrossComposition,
  type SubtitleTranslationCapabilities,
  type SubtitleTranslationJobContext,
  type SubtitleTranslationJobStatus,
  type SubtitleTranslationResultItem,
  type SubtitleTranslationSource,
} from './subtitleTranslationHelpers';

export type { SubtitleTranslationSource } from './subtitleTranslationHelpers';

interface UseSubtitleTranslationParams {
  t: Translations;
  projectResetKey?: string | null;
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
  selectedSubtitleIds: string[];
  editingSubtitleId: string | null;
  setActivePanel: (
    panel: 'zoom' | 'background' | 'cursor' | 'text' | 'subtitles',
  ) => void;
}


interface ResumableTranslationRun {
  jobId: string;
  jobContext: SubtitleTranslationJobContext;
  status: SubtitleTranslationJobStatus | null;
}

export function useSubtitleTranslation({
  t,
  projectResetKey,
  segment,
  setSegment,
  composition,
  setComposition,
  selectedSubtitleIds,
  editingSubtitleId,
  setActivePanel,
}: UseSubtitleTranslationParams) {
  const [targetLanguage, setTargetLanguage] = useState(getInitialTranslationLanguage);
  const [chunkCountOverride, setChunkCountOverride] = useState(getInitialTranslationChunkCount);
  const [isChunkSliderDragging, setIsChunkSliderDragging] = useState(false);
  const [instructions, setInstructions] = useState(getInitialTranslationInstructions);
  const [translationSource, setTranslationSource] = useState<SubtitleTranslationSource>(getInitialTranslationSource);
  const [selectedModelId, setSelectedModelId] = useState(getInitialTranslationModelId);
  const [smartFallback, setSmartFallback] = useState(getInitialSmartFallback);
  const [jobId, setJobId] = useState<string | null>(null);
  const [jobContext, setJobContext] = useState<SubtitleTranslationJobContext | null>(null);
  const [status, setStatus] = useState<SubtitleTranslationJobStatus | null>(null);
  const [capabilities, setCapabilities] = useState<SubtitleTranslationCapabilities | null>(null);
  const activeJobIdRef = useRef<string | null>(null);
  const lastProjectResetKeyRef = useRef<string | null | undefined>(undefined);
  const runNamespace = 'subtitle-translation';

  // Re-adopt an in-flight translation job after the panel remounts (tab switch).
  useResumableRun<ResumableTranslationRun>(runNamespace, jobId, (cached) => {
    setJobContext(cached.jobContext);
    setStatus(cached.status);
    setJobId(cached.jobId);
  });

  useEffect(() => {
    persistTranslationLanguage(targetLanguage);
  }, [targetLanguage]);

  useEffect(() => {
    persistTranslationChunkCount(chunkCountOverride);
  }, [chunkCountOverride]);

  useEffect(() => {
    persistTranslationInstructions(instructions);
  }, [instructions]);

  useEffect(() => {
    persistTranslationSource(translationSource);
  }, [translationSource]);

  useEffect(() => {
    persistTranslationModelId(selectedModelId);
  }, [selectedModelId]);

  useEffect(() => {
    persistTranslationSmartFallback(smartFallback);
  }, [smartFallback]);

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
      void invoke('cancel_subtitle_translation', { jobId: activeJobId }).catch(() => {});
    }
    clearResumableRun(runNamespace);
    setJobId(null);
    setJobContext(null);
    setStatus(null);
    setTranslationSource('current');
  }, [projectResetKey]);

  const refreshCapabilities = useCallback(async () => {
    const next = await invoke<SubtitleTranslationCapabilities>(
      'get_subtitle_translation_capabilities',
      {},
    );
    setCapabilities(next);
    return next;
  }, []);

  useEffect(() => {
    void refreshCapabilities().catch(() => {
      setCapabilities(null);
    });
  }, [refreshCapabilities]);

  useEffect(() => {
    if (!capabilities?.models.length) return;
    if (capabilities.models.some((model) => model.modelId === selectedModelId)) return;
    setSelectedModelId(capabilities.models[0]?.modelId ?? GTX_TRANSLATION_MODEL_ID);
  }, [capabilities, selectedModelId]);

  const subtitleTracks = useMemo(() => getSubtitleTracks(segment), [segment]);
  const activeSubtitleView = useMemo(() => getActiveSubtitleView(segment), [segment]);
  const activeTrack = useMemo(
    () => subtitleTracks.find((track) => track.id === activeSubtitleView.trackId) ?? null,
    [activeSubtitleView.trackId, subtitleTracks],
  );
  const isCustomView = activeSubtitleView.kind === 'custom';
  const isOriginalView = activeSubtitleView.kind === 'track' && activeSubtitleView.trackId === ORIGINAL_SUBTITLE_TRACK_ID;
  const canTranslate = capabilities?.available ?? true;
  const translationLanguageOptions = useMemo(
    () => SUBTITLE_LANGUAGE_OPTIONS.filter((option) => option.value !== 'auto'),
    [],
  );
  const translationModelOptions = useMemo(
    () => (capabilities?.models ?? []).map((model) => ({
      value: model.modelId,
      label: `${model.modelLabel} (${model.modelName})`,
      triggerLabel: model.modelLabel,
      keywords: [model.modelId, model.modelName, model.provider],
    })),
    [capabilities],
  );
  const targetLanguageTrack = useMemo(
    () => findTranslationTrackByLanguage(segment, targetLanguage),
    [segment, targetLanguage],
  );

  const updateCurrentSegment = useCallback((updater: (segment: VideoSegment) => VideoSegment) => {
    if (!segment) return;
    if (!composition || getEffectiveCompositionMode(composition) === 'separate') {
      setSegment((prev) => (prev ? updater(prev) : prev));
      return;
    }

    setComposition((prev) => {
      if (!prev) return prev;
      return updateSubtitleViewAcrossComposition(prev, updater);
    });
  }, [composition, segment, setComposition, setSegment]);

  const deleteSubtitleTrackById = useCallback((trackId: string) => {
    updateCurrentSegment((currentSegment) => removeSubtitleTrack(currentSegment, trackId));
  }, [updateCurrentSegment]);

  const subtitleViewOptions = useMemo(() => ([
    {
      value: ORIGINAL_SUBTITLE_TRACK_ID,
      label: t.subtitleTrackOriginal,
    },
    ...subtitleTracks
      .filter((track) => track.kind === 'translation')
      .map((track) => ({
        value: track.id,
        label: getSubtitleTrackLabel(track),
      })),
    {
      value: 'custom',
      label: t.subtitleTrackCustom,
    },
  ]), [subtitleTracks, t.subtitleTrackCustom, t.subtitleTrackOriginal]);

  const setSubtitleView = useCallback((value: string) => {
    setActivePanel('subtitles');
    updateCurrentSegment((currentSegment) =>
      value === 'custom'
        ? setActiveSubtitleCustomView(currentSegment)
        : setActiveSubtitleTrackView(currentSegment, value),
    );
  }, [setActivePanel, updateCurrentSegment]);

  const updateCustomChain = useCallback((chain: SubtitleChainItem[]) => {
    updateCurrentSegment((currentSegment) => setSubtitleCustomChain(currentSegment, chain));
  }, [updateCurrentSegment]);

  const selectedTranslationItems = useMemo(() => {
    if (!segment) return [];
    if (!composition || getEffectiveCompositionMode(composition) === 'separate') {
      return buildTranslationItems(segment, selectedSubtitleIds, editingSubtitleId, translationSource);
    }
    return buildCompositionTranslationItems(composition, selectedSubtitleIds, editingSubtitleId, translationSource);
  }, [composition, editingSubtitleId, segment, selectedSubtitleIds, translationSource]);
  const subtitleTranslationSourceCounts = useMemo(() => {
    const sources = new Set<SubtitleTranslationSource>([
      'current',
      'all',
      'audio',
      'video',
      'mic',
    ]);
    const entries = [...sources].map((source) => {
      const items = !segment
        ? []
        : !composition || getEffectiveCompositionMode(composition) === 'separate'
          ? buildTranslationItems(segment, selectedSubtitleIds, editingSubtitleId, source)
          : buildCompositionTranslationItems(composition, selectedSubtitleIds, editingSubtitleId, source);
      return [source, items.length] as const;
    });
    return Object.fromEntries(entries) as Partial<Record<SubtitleTranslationSource, number>>;
  }, [composition, editingSubtitleId, segment, selectedSubtitleIds]);
  const subtitleTranslationChunkMax = Math.max(1, selectedTranslationItems.length);
  const suggestedChunkCount = suggestTranslationChunkCount(selectedTranslationItems);
  const effectiveChunkCount = clampTranslationChunkCount(
    chunkCountOverride ?? suggestedChunkCount,
    subtitleTranslationChunkMax,
  );
  const subtitleTranslationChunkPreview = useMemo(
    () => (
      isChunkSliderDragging
        ? buildTranslationChunkPreview(selectedTranslationItems, effectiveChunkCount)
        : null
    ),
    [effectiveChunkCount, isChunkSliderDragging, selectedTranslationItems],
  );
  const setSubtitleTranslationChunkCount = useCallback((value: number) => {
    setChunkCountOverride(clampTranslationChunkCount(value, subtitleTranslationChunkMax));
  }, [subtitleTranslationChunkMax]);
  const resetSubtitleTranslationChunkCount = useCallback(() => {
    setChunkCountOverride(null);
  }, []);

  const applySubtitleTranslationResults = useCallback((
    results: SubtitleTranslationResultItem[],
    context: SubtitleTranslationJobContext,
  ) => {
    if (results.length === 0) return;
    if (!composition || getEffectiveCompositionMode(composition) === 'separate') {
      setSegment((prev) =>
        prev ? applyTranslationResultsToSegment(prev, results, context) : prev,
      );
      return;
    }

    setComposition((prev) => {
      if (!prev) return prev;
      const timeline = buildSequenceTimeline(prev);
      let next = prev;
      const resultsByClipId = new Map<string, SubtitleTranslationResultItem[]>();
      for (const result of results) {
        const clipId = result.clipId ?? 'root';
        const bucket = resultsByClipId.get(clipId) ?? [];
        bucket.push(result);
        resultsByClipId.set(clipId, bucket);
      }

      for (const clip of next.clips) {
        const clipResults = resultsByClipId.get(clip.id) ?? [];
        if (clipResults.length === 0) continue;
        const updatedSegment = applyTranslationResultsToSegment(
          clip.segment,
          clipResults,
          context,
        );
        next = updateCompositionClip(next, clip.id, { segment: updatedSegment });
        if (next.globalSegment && timeline) {
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

  useAsyncJobPoll<SubtitleTranslationJobStatus>({
    jobId,
    enabled: !!jobContext,
    fetchStatus: (activeJobId) =>
      invoke<SubtitleTranslationJobStatus>('get_subtitle_translation_status', {
        jobId: activeJobId,
      }),
    isTerminal: (nextStatus) =>
      nextStatus.state === 'completed'
      || nextStatus.state === 'cancelled'
      || nextStatus.state === 'error',
    onTick: (nextStatus) => {
      setStatus(nextStatus);
      if (jobId && jobContext) {
        saveResumableRun<ResumableTranslationRun>(runNamespace, {
          jobId,
          jobContext,
          status: nextStatus,
        });
      }
    },
    onComplete: (nextStatus) => {
      if (!jobContext) return;
      clearResumableRun(runNamespace);
      if (nextStatus.state === 'completed') {
        applySubtitleTranslationResults(nextStatus.results, jobContext);
        setJobId(null);
        setJobContext(null);
        setActivePanel('subtitles');
        return;
      }
      // cancelled or error terminal state
      if (nextStatus.state === 'error' && nextStatus.results.length > 0) {
        applySubtitleTranslationResults(nextStatus.results, jobContext);
        setActivePanel('subtitles');
      }
      setJobId(null);
      setJobContext(null);
    },
    onError: (error) => {
      clearResumableRun(runNamespace);
      setStatus({
        state: 'error',
        message: error instanceof Error ? error.message : t.subtitleTranslationStatusFailed,
        progress: 0,
        currentChunkCount: 0,
        currentChunkIndex: 0,
        totalChunks: 0,
        results: [],
        error: error instanceof Error ? error.message : String(error),
      });
      setJobId(null);
      setJobContext(null);
    },
    intervalFor: () => 450,
  });

  const handleTranslateSubtitles = useCallback(async () => {
    if (!segment) return;
    const latestCapabilities = await refreshCapabilities().catch(() => capabilities);
    if (latestCapabilities?.available === false) {
      const message = latestCapabilities.reason ?? t.subtitleTranslationUnavailable;
      setStatus({
        state: 'error',
        message,
        messageKey: 'subtitleTranslationUnavailable',
        progress: 0,
        currentChunkCount: 0,
        currentChunkIndex: 0,
        totalChunks: 0,
        results: [],
        error: message,
      });
      return;
    }

    const items = selectedTranslationItems;
    if (items.length === 0) {
      const message = t.subtitleTranslationNoSource;
      setStatus({
        state: 'error',
        message,
        messageKey: 'subtitleTranslationNoSource',
        progress: 0,
        currentChunkCount: 0,
        currentChunkIndex: 0,
        totalChunks: 0,
        results: [],
        error: message,
      });
      return;
    }

    const nextJobContext: SubtitleTranslationJobContext = {
      targetLanguage,
      targetTrackId: targetLanguageTrack?.id ?? null,
    };
    setJobContext(nextJobContext);
    setActivePanel('subtitles');
    const result = await invoke<{ jobId: string }>('start_subtitle_translation', {
      targetLanguage,
      trackId: targetLanguageTrack?.id ?? null,
      modelId: selectedModelId,
      smartFallback,
      chunkCount: effectiveChunkCount,
      instructions: instructions.trim() || null,
      items,
    });
    const queuedStatus: SubtitleTranslationJobStatus = {
      state: 'queued',
      message: t.subtitleTranslationStatusStarting,
      messageKey: 'subtitleTranslationStatusStarting',
      progress: 0,
      currentChunkCount: 0,
      currentChunkIndex: 0,
      totalChunks: 0,
      targetLanguage,
      results: [],
      error: null,
    };
    setStatus(queuedStatus);
    setJobId(result.jobId);
    saveResumableRun<ResumableTranslationRun>(runNamespace, {
      jobId: result.jobId,
      jobContext: nextJobContext,
      status: queuedStatus,
    });
  }, [
    capabilities,
    refreshCapabilities,
    segment,
    selectedTranslationItems,
    setActivePanel,
    targetLanguageTrack,
    selectedModelId,
    smartFallback,
    effectiveChunkCount,
    instructions,
    t.subtitleTranslationNoSource,
    t.subtitleTranslationStatusStarting,
    t.subtitleTranslationUnavailable,
    targetLanguage,
  ]);

  const handleCancelSubtitleTranslation = useCallback(
    buildCancelHandler({
      jobId,
      cancelCommand: 'cancel_subtitle_translation',
      onCancelled: () => {
        clearResumableRun(runNamespace);
        setStatus((prev) => (prev ? {
          ...prev,
          state: 'cancelled',
          message: t.subtitleTranslationStatusCancelled,
          messageKey: 'subtitleTranslationStatusCancelled',
          messageParams: {},
        } : prev));
        setJobId(null);
        setJobContext(null);
      },
    }),
    [jobId, t.subtitleTranslationStatusCancelled],
  );

  return {
    subtitleTracks,
    activeSubtitleView,
    activeTrack,
    subtitleViewOptions,
    subtitleCustomChain: segment?.subtitleCustomChain ?? [],
    visibleSubtitleSegments: getVisibleSubtitleSegments(segment),
    isCustomSubtitleView: isCustomView,
    isOriginalSubtitleView: isOriginalView,
    canGenerateSubtitlesFromCurrentView: !!segment,
    canCreateManualSubtitles: !!segment,
    subtitleTranslationTargetLanguage: targetLanguage,
    setSubtitleTranslationTargetLanguage: setTargetLanguage,
    subtitleTranslationChunkCount: effectiveChunkCount,
    subtitleTranslationChunkMax,
    subtitleTranslationSuggestedChunkCount: suggestedChunkCount,
    subtitleTranslationChunkCountIsAuto: chunkCountOverride === null,
    setSubtitleTranslationChunkCount,
    resetSubtitleTranslationChunkCount,
    setSubtitleTranslationChunkDragging: setIsChunkSliderDragging,
    subtitleTranslationChunkPreview,
    subtitleTranslationInstructions: instructions,
    setSubtitleTranslationInstructions: setInstructions,
    subtitleTranslationSource: translationSource,
    setSubtitleTranslationSource: setTranslationSource,
    subtitleTranslationSourceCounts,
    subtitleTranslationModelId: selectedModelId,
    setSubtitleTranslationModelId: setSelectedModelId,
    subtitleTranslationSmartFallback: smartFallback,
    setSubtitleTranslationSmartFallback: setSmartFallback,
    subtitleTranslationModelOptions: translationModelOptions,
    subtitleTranslationLanguageOptions: translationLanguageOptions,
    subtitleTranslationCapabilities: capabilities,
    canTranslateSubtitles: canTranslate && selectedTranslationItems.length > 0,
    hasExistingTranslationForTargetLanguage: !!targetLanguageTrack,
    subtitleTranslationStatusMessage: localizeStatus(t, status),
    isTranslatingSubtitles: !!jobId,
    setSubtitleView,
    updateSubtitleCustomChain: updateCustomChain,
    deleteSubtitleTrack: deleteSubtitleTrackById,
    handleTranslateSubtitles,
    handleCancelSubtitleTranslation,
  };
}
