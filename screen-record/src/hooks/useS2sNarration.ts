import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from 'react';
import { invoke } from '@/lib/ipc';
import { useAsyncJobPoll, buildCancelHandler } from './useAsyncJobPoll';
import { narrationProjectEpoch, narrationRunNamespace } from './narrationProjectOwnership';
import { newNarrationBatchId, registerNarrationBatch, retireNarrationBatch } from '@/lib/narrationBatchRetirements';
import { captureSubtitleSourceOwners, type SubtitleSourceOwner } from './subtitleSourceOwnership';
import { captureS2sDestination, s2sDestinationMatches, s2sSourcesCurrent, s2sSourceChangedStatus, retireStaleS2sRun, type S2sDestination } from './s2sSourceOwnership';
import { recordS2sSubtitleWrite, s2sSubtitleRevision, s2sSubtitlesCurrent, s2sSubtitleCoordinateSpace, s2sSubtitleDestination } from './s2sSubtitleOwnership';
import { getEffectiveCompositionMode } from '@/lib/projectComposition';
import { acknowledgeNarrationStart, recordNarrationResponse, recordNarrationStart, retireNarrationJob } from './retiredNarrationCancellation';
import {
  clearResumableRunIfJob,
  claimResumableStart,
  attachResumableStartCancellation,
  readResumableRun,
  readResumableStartMetadata,
  releaseResumableStart,
  saveResumableRun,
  useResumableStartPending,
} from './resumableJobRegistry';
import { useS2sRecovery } from './useS2sRecovery';
import type { UseS2sNarrationParams } from './s2sNarrationTypes';
import { S2S_COMMANDS, createS2sNarrationRunState, notifyS2sNarrationRun, trackS2sProgress, type ActiveS2sRun, type S2sNarrationRunState } from './s2sNarrationRunState';
import {
  buildSubtitleGenerationPlan,
  type SubtitleGenerationPlan,
} from '@/lib/subtitleGenerationPlan';
import type { SubtitleSegment } from '@/types/video';
import {
  collectS2sResults,
  cloneSubtitleSnapshot,
  localizeS2sStatus,
  replaceS2sSubtitleSegments,
  type S2sNarrationStatus,
  type S2sSubtitleStateSnapshot,
} from './s2sNarrationSubtitles';

export {
  populateEmptyS2sSubtitleTracks,
  type PopulateS2sSubtitleTracksOptions,
} from './s2sNarrationSubtitles';

export function useS2sNarration({
  getNarrationSourceSnapshot,
  projectId,
  currentProjectIdRef,
  backendMode = 's2s',
  t,
  duration,
  sourceType,
  selectedRange,
  targetLanguage,
  geminiModel,
  geminiVoice,
  geminiSpeed,
  parallelRequests,
  groupTextBudget,
  targetSegmentSec,
  onApplyNarrationSegments,
  onPopulateEmptySubtitles,
  onFinalize,
}: UseS2sNarrationParams) {
  const [jobId, setJobId] = useState<string | null>(null);
  const [status, setStatus] = useState<S2sNarrationStatus | null>(null);
  const [pollGeneration, setPollGeneration] = useState(0);
  const sharedRef = useRef<S2sNarrationRunState>(createS2sNarrationRunState());
  const revisionRef = useRef(0);
  const planRef = useRef<SubtitleGenerationPlan | null>(null);
  const sourceOwnersRef = useRef<Record<string, SubtitleSourceOwner>>({});
  const batchIdRef = useRef<string>('');
  const sourceSubtitleResultsRef = useRef<SubtitleSegment[]>([]);
  const targetSubtitleResultsRef = useRef<SubtitleSegment[]>([]);
  const baseSourceSubtitleResultsRef = useRef<SubtitleSegment[]>([]);
  const baseTargetSubtitleResultsRef = useRef<SubtitleSegment[]>([]);
  const baseSubtitleSnapshotRef = useRef<S2sSubtitleStateSnapshot | null>(null);
  const lastProgressSignatureRef = useRef('');
  const lastProgressAtRef = useRef(0);
  const lastStallLogAtRef = useRef(0);
  const jobTargetLanguageRef = useRef(targetLanguage);
  const latestRefs = useRef({
    t,
    onApplyNarrationSegments,
    onPopulateEmptySubtitles,
    onFinalize,
  });

  const ownerEpoch = narrationProjectEpoch(currentProjectIdRef);
  const runNamespace = narrationRunNamespace(projectId, 's2s', ownerEpoch);
  const isStarting = useResumableStartPending(runNamespace);
  const activeMode = readResumableRun<ActiveS2sRun>(runNamespace)?.backendMode
    ?? readResumableStartMetadata<'s2s' | 'gemini-translate'>(runNamespace)
    ?? backendMode;
  const isCurrentProject = useCallback(
    () => projectId !== null && currentProjectIdRef.current === projectId
      && narrationProjectEpoch(currentProjectIdRef) === ownerEpoch,
    [currentProjectIdRef, ownerEpoch, projectId],
  );

  const snapshotActiveRun = useCallback((activeJobId: string, nextStatus?: S2sNarrationStatus | null) => {
    if (!isCurrentProject()) return;
    const previous = readResumableRun<ActiveS2sRun>(runNamespace);
    if (previous?.jobId !== activeJobId || previous.shared !== sharedRef.current || previous.shared.retired
      || previous.resultsRevision > revisionRef.current) return;
    const plan = planRef.current;
    if (!plan) return;
    saveResumableRun<ActiveS2sRun>(runNamespace, {
      jobId: activeJobId,
      backendMode: activeMode,
      cancelCommand: S2S_COMMANDS[activeMode].cancel,
      shared: sharedRef.current,
      status: nextStatus ?? status,
      plan,
      sourceOwnersByClip: sourceOwnersRef.current,
      batchId: batchIdRef.current,
      targetLanguage: jobTargetLanguageRef.current,
      sourceSubtitleResults: sourceSubtitleResultsRef.current,
      targetSubtitleResults: targetSubtitleResultsRef.current,
      baseSourceSubtitleResults: baseSourceSubtitleResultsRef.current,
      baseTargetSubtitleResults: baseTargetSubtitleResultsRef.current,
      baseSubtitleSnapshot: baseSubtitleSnapshotRef.current,
      resultsRevision: revisionRef.current,
      lastProgressSignature: lastProgressSignatureRef.current,
      lastProgressAt: lastProgressAtRef.current,
      lastStallLogAt: lastStallLogAtRef.current,
    });
  }, [activeMode, isCurrentProject, runNamespace, status]);

  useS2sRecovery(runNamespace, jobId, isCurrentProject, {
    shared: sharedRef, plan: planRef, owners: sourceOwnersRef, batch: batchIdRef,
    language: jobTargetLanguageRef, source: sourceSubtitleResultsRef, target: targetSubtitleResultsRef,
    baseSource: baseSourceSubtitleResultsRef, baseTarget: baseTargetSubtitleResultsRef,
    snapshot: baseSubtitleSnapshotRef, revision: revisionRef, signature: lastProgressSignatureRef,
    progress: lastProgressAtRef, stall: lastStallLogAtRef,
  }, setStatus, setJobId);

  const shared = sharedRef.current;
  const subscribeShared = useCallback((listener: () => void) => {
    shared.listeners.add(listener);
    return () => { shared.listeners.delete(listener); };
  }, [shared]);
  const sharedFailed = useCallback(() => Boolean(shared.failure), [shared]);
  const applyRetryable = useSyncExternalStore(subscribeShared, sharedFailed, () => false);
  const isOwnedRun = useCallback((id: string | null, expected: S2sNarrationRunState) => {
    if (!id || !isCurrentProject() || expected.retired) return false;
    const saved = readResumableRun<ActiveS2sRun>(runNamespace);
    return saved?.jobId === id && saved.shared === expected;
  }, [isCurrentProject, runNamespace]);

  const sourcesCurrent = useCallback((plan: SubtitleGenerationPlan, owners: Record<string, SubtitleSourceOwner>, destination: S2sDestination | null = sharedRef.current.destination) => {
    return s2sSubtitlesCurrent(sharedRef.current, s2sSubtitleDestination(getNarrationSourceSnapshot(), plan))
      && s2sSourcesCurrent(getNarrationSourceSnapshot, plan, owners, destination);
  }, [getNarrationSourceSnapshot]);

  const rejectStaleRun = useCallback((id: string, expected: S2sNarrationRunState) => {
    if (!isOwnedRun(id, expected)) return;
    const saved = readResumableRun<ActiveS2sRun>(runNamespace);
    if (!saved) return;
    const message = s2sSubtitlesCurrent(expected, s2sSubtitleDestination(getNarrationSourceSnapshot(), saved.plan))
      && s2sDestinationMatches(getNarrationSourceSnapshot(), expected.destination)
      ? latestRefs.current.t.subtitleStatusSourceChanged : latestRefs.current.t.narrationS2sDestinationChanged;
    const terminal = retireStaleS2sRun(id, expected, saved, runNamespace, message, revisionRef.current);
    setStatus(terminal);
    setJobId((current) => current === id ? null : current);
  }, [getNarrationSourceSnapshot, isOwnedRun, runNamespace]);

  useEffect(() => {
    latestRefs.current = {
      t,
      onApplyNarrationSegments,
      onPopulateEmptySubtitles,
      onFinalize,
    };
  }, [onApplyNarrationSegments, onFinalize, onPopulateEmptySubtitles, t]);

  const reportApplyFailure = useCallback((error: unknown, expected: S2sNarrationRunState) => {
    const failure = error instanceof Error ? error : new Error(String(error));
    expected.failure = failure;
    notifyS2sNarrationRun(expected);
    if (isCurrentProject()) setStatus({
      state: 'error', message: failure.message, error: failure.message, progress: 0,
      totalClips: 0, completedClips: 0, resultsRevision: revisionRef.current, results: [],
    });
  }, [isCurrentProject]);

  const applyPending = useCallback(async (id: string, expected: S2sNarrationRunState) => {
    if (!isOwnedRun(id, expected)) return;
    if (expected.failure) throw expected.failure;
    const pending = expected.pending;
    if (!pending) return;
    if (!pending.running) {
      pending.running = (async () => {
        const saved = readResumableRun<ActiveS2sRun>(runNamespace);
        if (!saved || !isOwnedRun(id, expected)) return;
        if (!sourcesCurrent(saved.plan, saved.sourceOwnersByClip ?? {})) {
          rejectStaleRun(id, expected);
          return;
        }
        const latest = latestRefs.current;
        const updated = latest.onPopulateEmptySubtitles(saved.sourceSubtitleResults, saved.targetSubtitleResults, saved.targetLanguage, {
          coordinateSpace: s2sSubtitleCoordinateSpace(saved.plan),
          preserveExistingOutside: true,
          baseSourceSegments: saved.baseSourceSubtitleResults,
          baseTargetSegments: saved.baseTargetSubtitleResults,
          restoreSnapshot: saved.baseSubtitleSnapshot,
          debugPhase: 'live', liveUpdate: true,
        });
        recordS2sSubtitleWrite(expected, updated);
        if (!isOwnedRun(id, expected)) return;
        if (!sourcesCurrent(saved.plan, saved.sourceOwnersByClip ?? {})) {
          rejectStaleRun(id, expected);
          return;
        }
        await latest.onApplyNarrationSegments(pending.narrations, pending.replaceIds);
        if (!sourcesCurrent(saved.plan, saved.sourceOwnersByClip ?? {})) {
          rejectStaleRun(id, expected);
          return;
        }
        if (expected.pending === pending) expected.pending = null;
      })().catch((error: unknown) => {
        reportApplyFailure(error, expected);
        throw error;
      }).finally(() => { pending.running = null; });
    }
    await pending.running;
  }, [isOwnedRun, rejectStaleRun, reportApplyFailure, runNamespace, sourcesCurrent]);

  const finishRun = useCallback(async (id: string | null, expected: S2sNarrationRunState, completed: boolean, phase: string, terminalStatus?: S2sNarrationStatus) => {
    if (!id || !isOwnedRun(id, expected)) return false;
    if (!expected.completion) {
      expected.completion = (async () => {
        await applyPending(id, expected);
        const saved = readResumableRun<ActiveS2sRun>(runNamespace);
        if (!saved || !isOwnedRun(id, expected)) return;
        if (!sourcesCurrent(saved.plan, saved.sourceOwnersByClip ?? {})) {
          rejectStaleRun(id, expected);
          return;
        }
        const latest = latestRefs.current;
        const updated = latest.onPopulateEmptySubtitles(
          completed ? saved.sourceSubtitleResults : [], completed ? saved.targetSubtitleResults : [], saved.targetLanguage,
          completed ? {
            coordinateSpace: s2sSubtitleCoordinateSpace(saved.plan),
            preserveExistingOutside: true, baseSourceSegments: saved.baseSourceSubtitleResults,
            baseTargetSegments: saved.baseTargetSubtitleResults, restoreSnapshot: saved.baseSubtitleSnapshot, debugPhase: phase,
          } : { coordinateSpace: s2sSubtitleCoordinateSpace(saved.plan), restoreSnapshot: saved.baseSubtitleSnapshot, debugPhase: phase },
        );
        recordS2sSubtitleWrite(expected, updated);
        if (!isOwnedRun(id, expected)) return;
        if (!sourcesCurrent(saved.plan, saved.sourceOwnersByClip ?? {})) {
          rejectStaleRun(id, expected);
          return;
        }
        await latest.onFinalize();
        if (!sourcesCurrent(saved.plan, saved.sourceOwnersByClip ?? {})) {
          rejectStaleRun(id, expected);
          return;
        }
        if (isOwnedRun(id, expected)) {
          if (phase === 'poll-error') retireNarrationJob(saved.cancelCommand, id);
          else acknowledgeNarrationStart(saved.cancelCommand, id);
          expected.terminalStatus = terminalStatus ?? (phase === 'manual-cancel' && saved.status
            ? { ...saved.status, state: 'cancelled', message: 'Cancelled' }
            : saved.status);
          expected.retired = true;
          retireNarrationBatch(saved.batchId);
          clearResumableRunIfJob(runNamespace, id);
        }
      })().catch((error: unknown) => {
        expected.completion = null;
        reportApplyFailure(error, expected);
        throw error;
      });
    }
    try {
      await expected.completion;
      if (isCurrentProject() && expected.retired) setJobId((current) => current === id ? null : current);
      return expected.retired;
    } catch {
      return false;
    }
  }, [applyPending, isCurrentProject, isOwnedRun, rejectStaleRun, reportApplyFailure, runNamespace, sourcesCurrent]);

  useAsyncJobPoll<S2sNarrationStatus>({
    jobId,
    restartKey: `${activeMode}:${pollGeneration}`,
    fetchStatus: (activeJobId) =>
      invoke<S2sNarrationStatus>(S2S_COMMANDS[activeMode].status, {
        jobId: activeJobId,
        knownResultsRevision: revisionRef.current,
      }),
    isTerminal: (next) =>
      next.state === 'completed'
      || next.state === 'cancelled'
      || next.state === 'error',
    onTick: async (next) => {
      if (!jobId || !isOwnedRun(jobId, shared)) return;
      const owned = readResumableRun<ActiveS2sRun>(runNamespace);
      if (!owned || !sourcesCurrent(owned.plan, owned.sourceOwnersByClip ?? {})) {
        rejectStaleRun(jobId, shared);
        return;
      }
      await applyPending(jobId, shared);
      if (!isOwnedRun(jobId, shared)) return;
      const saved = readResumableRun<ActiveS2sRun>(runNamespace);
      if (saved && saved.resultsRevision > revisionRef.current) {
        revisionRef.current = saved.resultsRevision;
        sourceSubtitleResultsRef.current = saved.sourceSubtitleResults;
        targetSubtitleResultsRef.current = saved.targetSubtitleResults;
      }
      const alreadyProcessed = next.resultsRevision <= revisionRef.current;
      const commands = S2S_COMMANDS[activeMode];
      trackS2sProgress(next, jobId, commands.stallTag, lastProgressSignatureRef, lastProgressAtRef, lastStallLogAtRef);
      revisionRef.current = Math.max(revisionRef.current, next.resultsRevision);
      const latest = latestRefs.current;
      const displayStatus = {
        ...next,
        message: localizeS2sStatus(latest.t, next, activeMode),
        results: [],
      };
      setStatus(displayStatus);
      const plan = planRef.current;
      if (plan && next.results.length > 0 && !alreadyProcessed) {
        const { sourceSubtitles, targetSubtitles, narrations, replaceIds } =
          collectS2sResults(next, plan, batchIdRef.current, commands.ttsMethod, getNarrationSourceSnapshot().composition);
        sourceSubtitleResultsRef.current = replaceS2sSubtitleSegments([
          ...sourceSubtitleResultsRef.current,
          ...sourceSubtitles,
        ]);
        targetSubtitleResultsRef.current = replaceS2sSubtitleSegments([
          ...targetSubtitleResultsRef.current,
          ...targetSubtitles,
        ]);
        shared.pending = { narrations, replaceIds, running: null };
        snapshotActiveRun(jobId, displayStatus);
        await applyPending(jobId, shared);
      }
      if (!isOwnedRun(jobId, shared)) return;
      if (jobId) {
        snapshotActiveRun(jobId, displayStatus);
      }
    },
    onComplete: async (next) => {
      await finishRun(jobId, shared, next.state === 'completed', next.state);
    },
    onError: async (error) => {
      if (!isOwnedRun(jobId, shared)) return;
      const terminalStatus: S2sNarrationStatus = {
        state: 'error',
        message: error instanceof Error ? error.message : String(error),
        progress: 0,
        totalClips: 0,
        completedClips: 0,
        resultsRevision: revisionRef.current,
        results: [],
        error: error instanceof Error ? error.message : String(error),
      };
      setStatus(terminalStatus);
      if (!shared.failure) await finishRun(jobId, shared, false, 'poll-error', terminalStatus);
    },
    intervalFor: (next) => (next.results.length > 0 ? 250 : 600),
  });

  const handleGenerate = useCallback(async () => {
    if (jobId || isStarting || !isCurrentProject()) return;
    const source = getNarrationSourceSnapshot();
    if (getEffectiveCompositionMode(source.composition) === 'unified'
      || ((source.composition?.clips.length ?? 0) > 1 && (sourceType === 'audio' || sourceType.startsWith('audio:')))) {
      setStatus(s2sSourceChangedStatus(t.narrationS2sRequiresSingleClip)); return;
    }
    const destination = captureS2sDestination(source);
    if (!destination) { setStatus(s2sSourceChangedStatus(t.narrationS2sDestinationChanged)); return; }
    const plan = buildSubtitleGenerationPlan({
      segment: source.segment,
      composition: source.composition,
      activeClipId: source.loadedClipId,
      currentRawVideoPath: source.videoPath,
      currentRawMicAudioPath: source.micPath,
      duration,
      sourceType,
      selectedRange,
    });
    if (plan.clips.length === 0) {
      setStatus(s2sSourceChangedStatus(t.subtitleStatusNoSource));
      return;
    }
    const sourceOwners = captureSubtitleSourceOwners(plan, source.segment, source.composition);
    if (!s2sSourcesCurrent(getNarrationSourceSnapshot, plan, sourceOwners, destination)) {
      setStatus(s2sSourceChangedStatus(t.subtitleStatusSourceChanged));
      return;
    }
    const startClaim = claimResumableStart(runNamespace, backendMode);
    if (!startClaim) return;
    let pendingStartId: string | null = null;
    const commands = S2S_COMMANDS[backendMode];
    try {
      sharedRef.current = createS2sNarrationRunState(destination);
      sharedRef.current.subtitleRevisions = [s2sSubtitleRevision(s2sSubtitleDestination(source, plan))];
      planRef.current = plan;
      sourceOwnersRef.current = sourceOwners;
      batchIdRef.current = newNarrationBatchId(commands.batchPrefix);
      jobTargetLanguageRef.current = targetLanguage;
      sourceSubtitleResultsRef.current = [];
      targetSubtitleResultsRef.current = [];
      baseSubtitleSnapshotRef.current = cloneSubtitleSnapshot(s2sSubtitleDestination(source, plan));
      // Regenerating narration must FULLY REPLACE the prior run's cues, not merge
      // with them. Start from an empty base so the new cues populate cleanly;
      // otherwise the restore-snapshot at conclusion brings back the previous
      // run's stale subtitle (the audio updates but the subtitle reverts — the
      // streamed result diverging from the concluded one).
      baseSourceSubtitleResultsRef.current = [];
      baseTargetSubtitleResultsRef.current = [];
      revisionRef.current = 0;
      lastProgressSignatureRef.current = '';
      lastProgressAtRef.current = performance.now();
      lastStallLogAtRef.current = 0;
      pendingStartId = await recordNarrationStart(commands.cancel);
      if (!isCurrentProject()) {
        acknowledgeNarrationStart(commands.cancel, pendingStartId);
        pendingStartId = null;
        return;
      }
      if (!sourcesCurrent(plan, sourceOwners)) {
        acknowledgeNarrationStart(commands.cancel, pendingStartId);
        pendingStartId = null;
        setStatus(s2sSourceChangedStatus(s2sSubtitlesCurrent(sharedRef.current, s2sSubtitleDestination(getNarrationSourceSnapshot(), plan))
          && s2sDestinationMatches(getNarrationSourceSnapshot(), destination)
          ? t.subtitleStatusSourceChanged : t.narrationS2sDestinationChanged));
        return;
      }
      attachResumableStartCancellation(runNamespace, startClaim, commands.cancel, pendingStartId);
      const response = await invoke<{ jobId: string }>(commands.start, {
        jobId: pendingStartId,
        sourceType: plan.sourceTypeForNative,
        targetLanguage,
        geminiModel,
        geminiVoice,
        geminiSpeed,
        parallelRequests,
        groupTextBudget,
        targetSegmentSec,
        clips: plan.clips,
      });
      recordNarrationResponse(commands.cancel, pendingStartId, response.jobId);
      if (!isCurrentProject()) {
        retireNarrationJob(commands.cancel, response.jobId);
        pendingStartId = null;
        return;
      }
      if (!sourcesCurrent(plan, sourceOwners)) {
        retireNarrationJob(commands.cancel, response.jobId);
        pendingStartId = null;
        setStatus(s2sSourceChangedStatus(s2sSubtitlesCurrent(sharedRef.current, s2sSubtitleDestination(getNarrationSourceSnapshot(), plan))
          && s2sDestinationMatches(getNarrationSourceSnapshot(), destination)
          ? t.subtitleStatusSourceChanged : t.narrationS2sDestinationChanged));
        return;
      }
      setStatus({
        state: 'queued',
        message: backendMode === 'gemini-translate'
          ? t.narrationGeminiTranslateQueued
          : t.narrationS2sQueued,
        progress: 0,
        totalClips: plan.clips.length,
        completedClips: 0,
        resultsRevision: 0,
        results: [],
        error: null,
      });
      saveResumableRun<ActiveS2sRun>(runNamespace, {
        jobId: response.jobId,
        backendMode,
        cancelCommand: commands.cancel,
        shared: sharedRef.current,
        status: {
          state: 'queued',
          message: backendMode === 'gemini-translate'
            ? t.narrationGeminiTranslateQueued
            : t.narrationS2sQueued,
          progress: 0,
          totalClips: plan.clips.length,
          completedClips: 0,
          resultsRevision: 0,
          results: [],
          error: null,
        },
        plan,
        sourceOwnersByClip: sourceOwners,
        batchId: batchIdRef.current,
        targetLanguage: jobTargetLanguageRef.current,
        sourceSubtitleResults: sourceSubtitleResultsRef.current,
        targetSubtitleResults: targetSubtitleResultsRef.current,
        baseSourceSubtitleResults: baseSourceSubtitleResultsRef.current,
        baseTargetSubtitleResults: baseTargetSubtitleResultsRef.current,
        baseSubtitleSnapshot: baseSubtitleSnapshotRef.current,
        resultsRevision: revisionRef.current,
        lastProgressSignature: lastProgressSignatureRef.current,
        lastProgressAt: lastProgressAtRef.current,
        lastStallLogAt: lastStallLogAtRef.current,
      });
      if (projectId) registerNarrationBatch(projectId, batchIdRef.current,
        () => retireNarrationJob(commands.cancel, response.jobId));
      setJobId(response.jobId);
      pendingStartId = null;
    } catch (error) {
      if (pendingStartId) retireNarrationJob(commands.cancel, pendingStartId);
      if (!isCurrentProject()) return;
      setStatus({
        state: 'error',
        message: error instanceof Error ? error.message : String(error),
        progress: 0,
        totalClips: 0,
        completedClips: 0,
        resultsRevision: 0,
        results: [],
        error: error instanceof Error ? error.message : String(error),
      });
    } finally {
      releaseResumableStart(runNamespace, startClaim);
    }
  }, [
    backendMode,
    getNarrationSourceSnapshot,
    duration,
    geminiModel,
    geminiSpeed,
    geminiVoice,
    groupTextBudget,
    parallelRequests,
    isStarting,
    isCurrentProject,
    runNamespace,
    jobId,
    selectedRange,
    sourceType,
    t.narrationS2sQueued,
    t.narrationGeminiTranslateQueued,
    t.subtitleStatusNoSource,
    t.subtitleStatusSourceChanged,
    sourcesCurrent,
    targetLanguage,
  ]);

  const handleCancel = useCallback(
    buildCancelHandler({
      jobId,
      cancelCommand: S2S_COMMANDS[activeMode].cancel,
      onCancelled: async () => {
        if (!isOwnedRun(jobId, shared)) return;
        if (!(await finishRun(jobId, shared, false, 'manual-cancel'))) return;
        if (shared.terminalStatus?.state === 'error') return;
        if (!isCurrentProject()) return;
        setStatus((prev) => prev ? { ...prev, state: 'cancelled', message: 'Cancelled' } : prev);
      },
    }),
    [activeMode, finishRun, isCurrentProject, isOwnedRun, jobId, shared],
  );

  const handleRetry = useCallback(async () => {
    if (!jobId || !isOwnedRun(jobId, shared)) return;
    shared.failure = null;
    notifyS2sNarrationRun(shared);
    try {
      await applyPending(jobId, shared);
      if (isOwnedRun(jobId, shared)) setPollGeneration((value) => value + 1);
    } catch (error) {
      reportApplyFailure(error, shared);
    }
  }, [applyPending, isOwnedRun, jobId, reportApplyFailure, shared]);

  return {
    canGenerate: projectId !== null && !jobId && !isStarting,
    isGenerating: Boolean(jobId) || isStarting,
    applyRetryable,
    status,
    handleGenerate,
    handleCancel,
    handleRetry,
  };
}
