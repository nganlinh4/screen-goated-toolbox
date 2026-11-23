import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@/lib/ipc';
import {
  buildSubtitleGenerationPlan,
} from '@/lib/subtitleGenerationPlan';
import {
  type TrackSelectionRange,
} from '@/lib/timelineSegmentSelection';
import {
  buildAppliedResultsKey,
  buildSubtitleStatusViewKey,
  hasFinalSubtitleResult,
  localizeSubtitleStatus,
  splitGeneratedSubtitleResults,
  stripSubtitleJobResults,
  summarizeSubtitleRanges,
} from './subtitleGenerationResults';
import { useSubtitlePreferences } from './useSubtitlePreferences';
import type {
  ResumableSubtitleGenerationRun,
  SubtitleJobContext,
  SubtitleJobStatus,
  SubtitleJobViewStatus,
  UseSubtitleGenerationParams,
} from './subtitleGenerationTypes';
import { useSubtitleResultApplication } from './useSubtitleResultApplication';
import { useSubtitleSourceAvailability } from './useSubtitleSourceAvailability';
import { useAsyncJobPoll } from './useAsyncJobPoll';
import { useSubtitleGenerationCancellation } from './useSubtitleGenerationCancellation';
import { useSubtitleJobRestore } from './useSubtitleJobRestore';
import { useSubtitleMethod } from './useSubtitleMethod';
import { captureSubtitleSourceOwners } from './subtitleSourceOwnership';
import { retireSubtitlePendingStart, useSubtitleRecoveryHandlers } from './subtitleResumableRecovery';
import { requireSubtitleRecoveryClear, useSubtitleCompletionBarrier } from './subtitleRecoveryJournal';
import { hasPendingSubtitleCompletedSave, useSubtitleCompletedSaveState } from './subtitleCompletedPersistence';
import { isNarrationProjectLoading, narrationProjectEpoch } from './narrationProjectOwnership';
import { hasRetiringSubtitleGeneration, retireSubtitleGeneration } from './retiredSubtitleGeneration';
import { recorderShutdownPreparing } from '@/lib/audioImportWriteFence';
import { subtitleShutdownOverrideArmed } from './subtitleShutdown';
import { clearSubtitleStartTicket, readSubtitleStartTicket, saveSubtitleStartTicket } from './subtitleStartTicket';
import {
  clearResumableRunIfJob,
  claimResumableStart,
  ownsResumableStart,
  readResumableRun,
  readResumableStartMetadata,
  releaseResumableStart,
  saveResumableRun,
  useResumableStartPending,
} from './resumableJobRegistry';

export type { SubtitleMethod } from './subtitleGenerationTypes';

export function useSubtitleGeneration({
  t,
  projectResetKey,
  segment,
  segmentRef,
  currentProjectDataRef,
  currentProjectIdRef,
  currentRawVideoPathRef,
  currentRawMicAudioPathRef,
  setSegment,
  setSegmentSilently,
  composition,
  setComposition,
  setCompositionSilently,
  activeClipId,
  loadedClipIdRef,
  currentRawVideoPath,
  currentRawMicAudioPath,
  duration,
  setActivePanel,
  persistProject,
}: UseSubtitleGenerationParams) {
  const [editingSubtitleId, setEditingSubtitleId] = useState<string | null>(null);
  const {
    subtitleMethod, setSubtitleMethod, canUseSelectedSubtitleMethod, selectedSubtitleMethodReason,
    subtitleMethodCapabilities,
  } = useSubtitleMethod();
  const { sourceType, setSourceType, languageHint, setLanguageHint, geminiPrompt, setGeminiPrompt, groqVocabulary,
    setGroqVocabulary, autoSplitSubtitles, setAutoSplitSubtitles, autoSplitMaxUnits, setAutoSplitMaxUnits,
    autoSplitSubtitlesRef, autoSplitMaxUnitsRef } = useSubtitlePreferences();
  const [jobId, setJobId] = useState<string | null>(null);
  const [isStartingSubtitleJob, setIsStartingSubtitleJob] = useState(false);
  const [jobContext, setJobContext] = useState<SubtitleJobContext | null>(null);
  const [status, setStatus] = useState<SubtitleJobViewStatus | null>(null);
  const lastAppliedResultsKeyRef = useRef('');
  const lastKnownResultsRevisionRef = useRef(0);
  const lastStatusViewKeyRef = useRef('');
  const staleResultsRef = useRef(false);
  const activeJobIdRef = useRef<string | null>(null);
  const cancelledJobIdRef = useRef<string | null>(null);
  const startEpochRef = useRef(0);
  const startingEpochRef = useRef<number | null>(null);
  const startClaimRef = useRef<symbol | null>(null);
  const retainCompletion = useSubtitleCompletionBarrier();
  const lastProjectResetKeyRef = useRef<string | null | undefined>(undefined);
  const currentProjectKeyRef = useRef(projectResetKey ?? null);
  currentProjectKeyRef.current = projectResetKey ?? null;
  const activeProjectKeyRef = useRef<string | null>(null);
  const activeProjectEpochRef = useRef(0);
  const projectEpoch = currentProjectIdRef ? narrationProjectEpoch(currentProjectIdRef) : 0;
  const ownsProjectEpoch = useCallback(() => (!currentProjectIdRef || !isNarrationProjectLoading(currentProjectIdRef))
    && activeProjectEpochRef.current === (currentProjectIdRef ? narrationProjectEpoch(currentProjectIdRef) : 0), [currentProjectIdRef]);
  const lastProjectEpochRef = useRef(projectEpoch);
  const runNamespace = 'subtitle-generation';
  const completedSaveState = useSubtitleCompletedSaveState();
  const isStartingShared = useResumableStartPending(runNamespace);
  const cancelPendingStart = useCallback(() => retireSubtitlePendingStart(), []);
  const { onStaleResults, onQueuedResultsFlushed } = useSubtitleRecoveryHandlers(staleResultsRef, activeJobIdRef, activeProjectKeyRef);

  useEffect(() => {
    if (!isStartingShared) return;
    const pending = readResumableStartMetadata<{ projectKey: string | null; projectEpoch: number }>(runNamespace);
    if (!pending) return;
    const localClaim = startClaimRef.current;
    const locallyOwned = localClaim !== null && ownsResumableStart(runNamespace, localClaim);
    if (projectResetKey == null && pending.projectKey !== null && !locallyOwned) return;
    if (pending.projectKey === (projectResetKey ?? null) && pending.projectEpoch === projectEpoch && (pending.projectKey !== null || locallyOwned)) return;
    cancelPendingStart();
  }, [cancelPendingStart, isStartingShared, projectResetKey, projectEpoch]);

  const {
    clearQueuedSubtitleResults, flushQueuedSubtitleResults, getQueuedSubtitleResults,
    markCompletedJobForPersist, queueSubtitleResults,
  } = useSubtitleResultApplication({
    segment,
    segmentRef,
    currentProjectDataRef,
    currentProjectIdRef,
    projectResetKey,
    currentRawVideoPathRef,
    currentRawMicAudioPathRef,
    currentRawVideoPath,
    currentRawMicAudioPath,
    onStaleResults, onQueuedResultsFlushed, ownsProjectEpoch,
    setSegment,
    setSegmentSilently,
    loadedClipIdRef,
    composition,
    setComposition,
    setCompositionSilently,
    jobContext,
    jobId,
    persistProject,
  });

  useSubtitleJobRestore({ jobId, projectResetKey, projectEpoch, lastKnownResultsRevisionRef, lastAppliedResultsKeyRef,
    lastStatusViewKeyRef, staleResultsRef, activeJobIdRef, activeProjectKeyRef, activeProjectEpochRef,
    setJobContext, setStatus, setJobId, queueSubtitleResults });

  useEffect(() => {
    activeJobIdRef.current = jobId;
  }, [jobId]);

  useEffect(() => {
    const nextKey = projectResetKey ?? null;
    if (lastProjectResetKeyRef.current === undefined) {
      if (nextKey === null && !activeJobIdRef.current && startingEpochRef.current === null) return;
      if ((!activeJobIdRef.current && startingEpochRef.current === null) || activeProjectKeyRef.current === nextKey) {
        lastProjectResetKeyRef.current = nextKey;
        return;
      }
    } else if (lastProjectResetKeyRef.current === nextKey && lastProjectEpochRef.current === projectEpoch) {
      return;
    }
    lastProjectResetKeyRef.current = nextKey;
    lastProjectEpochRef.current = projectEpoch;

    const activeJobId = activeJobIdRef.current;
    if (activeJobId) {
      const run = readResumableRun<ResumableSubtitleGenerationRun>(runNamespace);
      if (run?.jobId === activeJobId) retireSubtitleGeneration(run);
    }
    if (activeJobId) clearResumableRunIfJob(runNamespace, activeJobId);
    cancelPendingStart();
    setEditingSubtitleId(null);
    startEpochRef.current += 1;
    startingEpochRef.current = null;
    activeJobIdRef.current = null;
    setJobId(null);
    setIsStartingSubtitleJob(false);
    setJobContext(null);
    setStatus(null);
    lastAppliedResultsKeyRef.current = '';
    lastKnownResultsRevisionRef.current = 0;
    lastStatusViewKeyRef.current = '';
    staleResultsRef.current = false;
    clearQueuedSubtitleResults();
  }, [cancelPendingStart, clearQueuedSubtitleResults, projectResetKey, projectEpoch]);

  const {
    canUseAudioSource,
    canUseMicSource,
    canUseVideoSource,
  } = useSubtitleSourceAvailability({
    composition,
    currentRawMicAudioPath,
    currentRawVideoPath,
    setSourceType,
    sourceType,
  });

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
      if (!jobId || !ownsProjectEpoch() || activeJobIdRef.current !== jobId || cancelledJobIdRef.current === jobId || activeProjectKeyRef.current !== currentProjectKeyRef.current || readResumableRun<ResumableSubtitleGenerationRun>(runNamespace)?.jobId !== jobId) return;
      const nextViewStatus = nextStatus.state === 'completed'
        ? { ...stripSubtitleJobResults(nextStatus), state: 'running' as const, message: t.subtitleStatusSavingRecovery, messageKey: 'subtitleStatusSavingRecovery' }
        : stripSubtitleJobResults(nextStatus);
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
      if (generatedResults.length > 0) {
        // Event revisions acknowledge delivery, even when the presentation key
        // is unchanged. Otherwise the worker replays that event indefinitely.
        lastKnownResultsRevisionRef.current = Math.max(lastKnownResultsRevisionRef.current, nextStatus.resultsRevision);
      }
      if (
        generatedResults.length > 0
        && nextAppliedResultsKey !== lastAppliedResultsKeyRef.current
      ) {
        if (nextStatus.state === 'completed') {
          clearQueuedSubtitleResults();
        }
        lastAppliedResultsKeyRef.current = nextAppliedResultsKey;
        queueSubtitleResults(generatedResults, hasFinalSubtitleResult(generatedResults));
      } else if (generatedResults.length > 0 && generatedResults.every((result) => result.isPartial) && getQueuedSubtitleResults().length > 0) {
        // Keep the newest payload in an already pending batch even when its
        // presentation key deliberately ignores live tail timing changes.
        queueSubtitleResults(generatedResults, false);
      }
      if (jobId) {
        saveResumableRun<ResumableSubtitleGenerationRun>(runNamespace, {
          jobId,
          projectKey: activeProjectKeyRef.current,
          projectEpoch: activeProjectEpochRef.current,
          jobContext,
          status: nextViewStatus,
          lastKnownResultsRevision: lastKnownResultsRevisionRef.current,
          lastAppliedResultsKey: lastAppliedResultsKeyRef.current,
          lastStatusViewKey: lastStatusViewKeyRef.current,
          staleResults: staleResultsRef.current,
          pendingResults: getQueuedSubtitleResults(),
        });
      }
    },
    onComplete: async (nextStatus) => {
      if (!jobId || !ownsProjectEpoch() || activeJobIdRef.current !== jobId || cancelledJobIdRef.current === jobId || activeProjectKeyRef.current !== currentProjectKeyRef.current || readResumableRun<ResumableSubtitleGenerationRun>(runNamespace)?.jobId !== jobId) return;
      if (nextStatus.state === 'completed' && nextStatus.results.length) {
        const completedEpoch = activeProjectEpochRef.current;
        const recovery = await retainCompletion({ version: 1, jobId, projectId: activeProjectKeyRef.current, context: jobContext, results: nextStatus.results });
        if (recovery === 'unmounted') return;
        if (recovery === 'discarded' || recovery === 'discarded-unmounted') {
          clearSubtitleStartTicket(jobId);
          const resumed = readResumableRun<ResumableSubtitleGenerationRun>(runNamespace);
          if (resumed?.jobId === jobId && resumed.projectKey === jobContext?.projectKey && resumed.projectEpoch === completedEpoch) clearResumableRunIfJob(runNamespace, jobId);
          if (recovery === 'discarded-unmounted') return;
          if (activeProjectEpochRef.current !== completedEpoch || !ownsProjectEpoch() || activeJobIdRef.current !== jobId || activeProjectKeyRef.current !== currentProjectKeyRef.current) return;
          clearQueuedSubtitleResults();
          activeJobIdRef.current = null;
          setJobId(null);
          setJobContext(null);
          setStatus({ ...stripSubtitleJobResults(nextStatus), state: 'cancelled',
            message: t.subtitleStatusCancelled, messageKey: 'subtitleStatusCancelled', error: null });
          return;
        }
        if (activeProjectEpochRef.current !== completedEpoch || !ownsProjectEpoch() || activeJobIdRef.current !== jobId || cancelledJobIdRef.current === jobId || activeProjectKeyRef.current !== currentProjectKeyRef.current || readResumableRun<ResumableSubtitleGenerationRun>(runNamespace)?.jobId !== jobId) return;
      }
      clearSubtitleStartTicket(jobId);
      clearResumableRunIfJob(runNamespace, jobId);
      if (nextStatus.state === 'completed') {
        flushQueuedSubtitleResults();
        markCompletedJobForPersist(staleResultsRef.current);
        if (staleResultsRef.current) {
          setStatus({ ...stripSubtitleJobResults(nextStatus), state: 'error',
            message: t.subtitleStatusSourceChanged, messageKey: 'subtitleStatusSourceChanged',
            error: t.subtitleStatusSourceChanged });
        } else {
          setStatus(stripSubtitleJobResults(nextStatus));
        }
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
      if (!jobId || !ownsProjectEpoch() || activeJobIdRef.current !== jobId || cancelledJobIdRef.current === jobId || activeProjectKeyRef.current !== currentProjectKeyRef.current || readResumableRun<ResumableSubtitleGenerationRun>(runNamespace)?.jobId !== jobId) return;
      const run = readResumableRun<ResumableSubtitleGenerationRun>(runNamespace);
      if (run?.jobId === jobId) void retireSubtitleGeneration(run);
      clearResumableRunIfJob(runNamespace, jobId);
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
    if (recorderShutdownPreparing() || subtitleShutdownOverrideArmed()) return;
    if (currentProjectIdRef && isNarrationProjectLoading(currentProjectIdRef)) return;
    if (hasPendingSubtitleCompletedSave() || hasRetiringSubtitleGeneration()) {
      setStatus({ state: 'error', message: t.subtitleStatusSavePending, messageKey: 'subtitleStatusSavePending',
        progress: 0, activeClipId: null, totalClips: 0, completedClips: 0,
        resultsRevision: 0, skipped: [], error: t.subtitleStatusSavePending });
      return;
    }
    if (jobId || isStartingSubtitleJob || isStartingShared) {
      return;
    }
    const ownerProjectKey = projectResetKey ?? null;
    const ownerProjectEpoch = currentProjectIdRef ? narrationProjectEpoch(currentProjectIdRef) : 0;
    const claim = claimResumableStart(runNamespace, { projectKey: ownerProjectKey, projectEpoch: ownerProjectEpoch });
    if (!claim) return;
    startClaimRef.current = claim;
    const startEpoch = ++startEpochRef.current;
    staleResultsRef.current = false;
    const ownsStart = () => ownsResumableStart(runNamespace, claim)
      && startEpoch === startEpochRef.current && ownerProjectKey === currentProjectKeyRef.current
      && (!currentProjectIdRef || !isNarrationProjectLoading(currentProjectIdRef))
      && ownerProjectEpoch === (currentProjectIdRef ? narrationProjectEpoch(currentProjectIdRef) : 0);
    activeProjectKeyRef.current = ownerProjectKey;
    activeProjectEpochRef.current = ownerProjectEpoch;
    startingEpochRef.current = startEpoch;
    setIsStartingSubtitleJob(true);
    let startTicket: ResumableSubtitleGenerationRun | null = null;
    try {
    await requireSubtitleRecoveryClear(t.subtitleRecoveryUnavailable, t.subtitleStatusSavePending);
    if (!ownsStart()) return;
    if (readSubtitleStartTicket()) throw new Error(t.subtitleStatusSavePending);
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
      projectKey: ownerProjectKey,
      replacementRangesByClip: plan.replacementRangesByClip,
      indicator: plan.indicator,
      sourceTypeForNative: plan.sourceTypeForNative,
      clipTransformsByClip: plan.clipTransformsByClip,
      sourceOwnersByClip: captureSubtitleSourceOwners(plan, segment, composition),
    };
    setJobContext(nextJobContext);

    const allocated = await invoke<{ jobId: string }>('allocate_narration_job_id', { kind: 'transcription' });
    if (!ownsStart()) return;
    if (!/^ticket-[0-9a-f]{32}-transcription-[1-9][0-9]*$/.test(allocated?.jobId)) throw new Error('Subtitle start identity is unavailable');
    startTicket = { jobId: allocated.jobId, projectKey: ownerProjectKey,
      projectEpoch: ownerProjectEpoch, jobContext: nextJobContext, status: null, lastKnownResultsRevision: 0,
      lastAppliedResultsKey: '', lastStatusViewKey: '' };
    saveSubtitleStartTicket(startTicket);
    const result = await invoke<{ jobId: string }>('start_subtitle_generation', {
      jobId: allocated.jobId,
      sourceType: plan.sourceTypeForNative,
      subtitleMethod,
      languageHint: languageHint.trim() || 'auto',
      geminiPrompt: geminiPrompt.trim() || null,
      groqVocabulary,
      clips: plan.clips,
    });
    if (result.jobId !== allocated.jobId) {
      void retireSubtitleGeneration({ ...startTicket, jobId: result.jobId });
      void retireSubtitleGeneration(startTicket);
      throw new Error('Subtitle worker did not accept the start identity');
    }
    if (!ownsStart()) {
      // A history restore or project change retired this start while the
      // worker was accepting it. Never adopt its results into the editor.
      retireSubtitleGeneration({ jobId: result.jobId, projectKey: ownerProjectKey, projectEpoch: ownerProjectEpoch,
        jobContext: nextJobContext, status: null, lastKnownResultsRevision: 0,
        lastAppliedResultsKey: '', lastStatusViewKey: '' });
      return;
    }

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
    cancelledJobIdRef.current = null;
    activeJobIdRef.current = result.jobId;
    activeProjectKeyRef.current = ownerProjectKey;
    setJobId(result.jobId);
    saveResumableRun<ResumableSubtitleGenerationRun>(runNamespace, {
      jobId: result.jobId,
      projectKey: ownerProjectKey,
      projectEpoch: ownerProjectEpoch,
      jobContext: nextJobContext,
      status: queuedStatus,
      lastKnownResultsRevision: 0,
      lastAppliedResultsKey: '',
      lastStatusViewKey: '',
      staleResults: false, pendingResults: [],
    });
    } catch (error) {
      if (startTicket) void retireSubtitleGeneration(startTicket);
      if (!ownsStart()) return;
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
      releaseResumableStart(runNamespace, claim);
      if (startClaimRef.current === claim) startClaimRef.current = null;
      if (startingEpochRef.current === startEpoch) {
        startingEpochRef.current = null;
        setIsStartingSubtitleJob(false);
      }
    }
  }, [
    activeClipId,
    clearQueuedSubtitleResults,
    composition,
    currentProjectIdRef,
    currentRawMicAudioPath,
    currentRawVideoPath,
    duration,
    isStartingSubtitleJob,
    isStartingShared,
    jobId,
    geminiPrompt,
    groqVocabulary,
    languageHint,
    projectResetKey,
    segment,
    setActivePanel,
    sourceType,
    subtitleMethod,
    t.subtitleStatusSavePending,
  ]);

  const { handleCancelSubtitleGeneration, retireSubtitleForHistory } = useSubtitleGenerationCancellation({
    activeJobIdRef, cancelledJobIdRef, startEpochRef, startingEpochRef,
    lastAppliedResultsKeyRef, lastKnownResultsRevisionRef, lastStatusViewKeyRef,
    clearQueuedSubtitleResults, cancelPendingStart, setStatus, setJobId, setJobContext,
    setIsStartingSubtitleJob, cancelledMessage: t.subtitleStatusCancelled,
    unconfirmedMessage: t.subtitleStatusCancelUnconfirmed,
  });

  return {
    editingSubtitleId,
    setEditingSubtitleId,
    subtitleSource: sourceType,
    setSubtitleSource: setSourceType,
    subtitleMethod,
    setSubtitleMethod,
    subtitleMethodCapabilities,
    canUseSelectedSubtitleMethod,
    selectedSubtitleMethodReason,
    subtitleLanguageHint: languageHint,
    setSubtitleLanguageHint: setLanguageHint,
    subtitleGeminiPrompt: geminiPrompt,
    setSubtitleGeminiPrompt: setGeminiPrompt,
    subtitleGroqVocabulary: groqVocabulary,
    setSubtitleGroqVocabulary: setGroqVocabulary,
    isGeneratingSubtitles: !!jobId || isStartingSubtitleJob || isStartingShared,
    subtitleStatusMessage: completedSaveState === 'unconfirmed' || completedSaveState === 'superseded'
      ? t.subtitleStatusSaveUnconfirmed : localizeSubtitleStatus(t, status),
    subtitleActiveClipId: status?.activeClipId ?? null,
    subtitleGenerationIndicator: jobContext?.indicator ?? null,
    canUseVideoSubtitleSource: canUseVideoSource,
    canUseMicSubtitleSource: canUseMicSource,
    canUseAudioSubtitleSource: canUseAudioSource,
    handleGenerateSubtitles,
    handleCancelSubtitleGeneration,
    retireSubtitleForHistory,
    autoSplitSubtitles,
    setAutoSplitSubtitles,
    autoSplitMaxUnits,
    setAutoSplitMaxUnits,
  };
}
