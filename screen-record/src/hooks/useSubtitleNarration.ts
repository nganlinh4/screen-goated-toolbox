import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore } from 'react';
import type { Translations } from '@/i18n';
import { invoke } from '@/lib/ipc';
import { useAsyncJobPoll, buildCancelHandler } from './useAsyncJobPoll';
import { narrationProjectEpoch, narrationRunNamespace } from './narrationProjectOwnership';
import { newNarrationBatchId, registerNarrationBatch, retireNarrationBatch } from '@/lib/narrationBatchRetirements';
import { acknowledgeNarrationStart, recordNarrationResponse, recordNarrationStart, retireNarrationJob } from './retiredNarrationCancellation';
import { createSubtitleNarrationApplyQueue, notifySubtitleNarrationQueue, settleSubtitleNarrationDrain, type SubtitleNarrationApplyQueue } from './subtitleNarrationApplyQueue';
import {
  clearResumableRunIfJob,
  claimResumableStart,
  attachResumableStartCancellation,
  hasResumableStart,
  releaseResumableStart,
  readResumableRun,
  saveResumableRun,
  useResumableStartPending,
  useResumableRun,
} from './resumableJobRegistry';
import type { TrackSelectionRange } from '@/lib/timelineSegmentSelection';
import { useSubtitleNarrationTargets } from './useSubtitleNarrationTargets';
import type {
  NarrationSegment,
  SubtitleSegment,
} from '@/types/video';
import type { NarrationProfilePayload } from '@/hooks/useNarrationSettings';
import {
  buildNarrationGroupPreview,
  buildNarrationSegment,
  countNarrationOverlaps,
  DEFAULT_NARRATION_GROUP_TEXT_BUDGET,
  NARRATION_GROUP_VAD_RADIUS_SEC,
  type SubtitleNarrationJobStatus,
  type SubtitleNarrationResultItem,
} from './subtitleNarrationHelpers';

export {
  DEFAULT_NARRATION_GROUP_TEXT_BUDGET,
  DEFAULT_NARRATION_TARGET_SEGMENT_SEC,
  MAX_NARRATION_GROUP_TEXT_BUDGET,
  MAX_NARRATION_TARGET_SEGMENT_SEC,
  MIN_NARRATION_GROUP_TEXT_BUDGET,
  MIN_NARRATION_TARGET_SEGMENT_SEC,
  type SubtitleNarrationGroupPreview,
} from './subtitleNarrationHelpers';

const APPLY_RESULT_STREAM_INTERVAL_MS = 140;
const STATUS_UI_UPDATE_INTERVAL_MS = 900;

interface ResumableSubtitleNarrationRun {
  jobId: string;
  status: SubtitleNarrationJobStatus | null;
  batchId: string | null;
  applyQueue: SubtitleNarrationApplyQueue;
  knownResultsRevision: number;
  allResultItems: SubtitleNarrationResultItem[];
  allErrorItems: Array<{ subtitleId: string; message: string }>;
}

interface UseSubtitleNarrationParams {
  projectId: string | null;
  currentProjectIdRef: React.MutableRefObject<string | null>;
  t: Translations;
  visibleSubtitles: SubtitleSegment[];
  projectSubtitle?: (subtitle: SubtitleSegment) => SubtitleSegment | null;
  selectedSubtitleIds?: string[];
  selectedSubtitleRange?: TrackSelectionRange | null;
  sourceLanguageCode?: string | null;
  profile: NarrationProfilePayload;
  readUnsplitSubtitles?: boolean;
  groupTextBudget?: number;
  previewGrouping?: boolean;
  onApplyNarrationSegments: (
    segments: NarrationSegment[],
    replaceSubtitleIds: string[],
  ) => void | Promise<void>;
  onFinalizeNarrationSegments: () => void | Promise<void>;
}

export function useSubtitleNarration({
  projectId,
  currentProjectIdRef,
  t,
  visibleSubtitles, projectSubtitle,
  selectedSubtitleIds,
  selectedSubtitleRange,
  sourceLanguageCode,
  profile,
  readUnsplitSubtitles = true,
  groupTextBudget = DEFAULT_NARRATION_GROUP_TEXT_BUDGET,
  previewGrouping = false,
  onApplyNarrationSegments,
  onFinalizeNarrationSegments,
}: UseSubtitleNarrationParams) {
  const [jobId, setJobId] = useState<string | null>(null);
  const [status, setStatus] = useState<SubtitleNarrationJobStatus | null>(null);
  const [pollGeneration, setPollGeneration] = useState(0);
  const batchIdRef = useRef<string | null>(null);
  const applyQueueRef = useRef<SubtitleNarrationApplyQueue>(createSubtitleNarrationApplyQueue());
  const knownResultsRevisionRef = useRef(0);
  const allResultItemsRef = useRef<SubtitleNarrationResultItem[]>([]);
  const allErrorItemsRef = useRef<Array<{ subtitleId: string; message: string }>>([]);
  const pendingStatusTimerRef = useRef<number | null>(null);
  const lastStatusUiCommitAtRef = useRef(0);
  const latestStatusRef = useRef<SubtitleNarrationJobStatus | null>(null);
  const profileRef = useRef<NarrationProfilePayload>(profile);
  const onApplyNarrationSegmentsRef = useRef(onApplyNarrationSegments);
  useEffect(() => {
    profileRef.current = profile;
  }, [profile]);
  useEffect(() => {
    onApplyNarrationSegmentsRef.current = onApplyNarrationSegments;
  }, [onApplyNarrationSegments]);

  const ownerEpoch = narrationProjectEpoch(currentProjectIdRef);
  const runNamespace = narrationRunNamespace(projectId, 'subtitle', ownerEpoch);
  const isStarting = useResumableStartPending(runNamespace);
  const isCurrentProject = useCallback(
    () => projectId !== null && currentProjectIdRef.current === projectId
      && narrationProjectEpoch(currentProjectIdRef) === ownerEpoch,
    [currentProjectIdRef, ownerEpoch, projectId],
  );

  // Re-adopt an in-flight narration job after the panel remounts (tab switch).
  useResumableRun<ResumableSubtitleNarrationRun>(runNamespace, jobId, (cached) => {
    if (!isCurrentProject()) return;
    batchIdRef.current = cached.batchId;
    applyQueueRef.current = cached.applyQueue;
    knownResultsRevisionRef.current = cached.knownResultsRevision;
    allResultItemsRef.current = cached.allResultItems;
    allErrorItemsRef.current = cached.allErrorItems;
    setStatus(cached.status);
    setJobId(cached.jobId);
  }, (retiredJobId) => {
    const queue = applyQueueRef.current;
    if (!isCurrentProject() || !queue.retired) return;
    if (queue.terminalStatus && !hasResumableStart(runNamespace)) setStatus(queue.terminalStatus);
    setJobId((current) => current === retiredJobId ? null : current);
  });

  const currentApplyQueue = applyQueueRef.current;
  const subscribeQueue = useCallback((listener: () => void) => {
    currentApplyQueue.listeners.add(listener);
    return () => { currentApplyQueue.listeners.delete(listener); };
  }, [currentApplyQueue]);
  const queueHasFailure = useCallback(() => Boolean(currentApplyQueue.failure), [currentApplyQueue]);
  const applyRetryable = useSyncExternalStore(subscribeQueue, queueHasFailure, () => false);

  const { narrationTargetById, requestItems } = useSubtitleNarrationTargets(
    visibleSubtitles, selectedSubtitleIds, selectedSubtitleRange, readUnsplitSubtitles, projectSubtitle,
  );
  const narrationGroupPreview = useMemo(
    () => (
      previewGrouping
        ? buildNarrationGroupPreview(requestItems, groupTextBudget)
        : null
    ),
    [groupTextBudget, previewGrouping, requestItems],
  );

  const finalizeNarration = useCallback(async () => {
    if (!isCurrentProject()) return;
    await onFinalizeNarrationSegments();
  }, [isCurrentProject, onFinalizeNarrationSegments]);

  const publishStatus = useCallback((
    nextStatus: SubtitleNarrationJobStatus,
    options: { immediate?: boolean } = {},
  ) => {
    latestStatusRef.current = nextStatus;
    if (options.immediate) {
      if (pendingStatusTimerRef.current !== null) {
        window.clearTimeout(pendingStatusTimerRef.current);
        pendingStatusTimerRef.current = null;
      }
      lastStatusUiCommitAtRef.current = performance.now();
      setStatus(nextStatus);
      return;
    }

    const now = performance.now();
    const elapsed = now - lastStatusUiCommitAtRef.current;
    if (elapsed >= STATUS_UI_UPDATE_INTERVAL_MS) {
      lastStatusUiCommitAtRef.current = now;
      setStatus(nextStatus);
      return;
    }
    if (pendingStatusTimerRef.current !== null) return;
    pendingStatusTimerRef.current = window.setTimeout(() => {
      pendingStatusTimerRef.current = null;
      const latest = latestStatusRef.current;
      if (!latest) return;
      lastStatusUiCommitAtRef.current = performance.now();
      setStatus(latest);
    }, STATUS_UI_UPDATE_INTERVAL_MS - elapsed);
  }, []);

  const scheduleApplyDrain = useCallback(() => {
    const queue = applyQueueRef.current;
    if (queue.timer !== null || queue.applying || queue.failure) return;
    queue.timer = window.setTimeout(async () => {
      queue.timer = null;
      if (!isCurrentProject()) {
        queue.pending = [];
        settleSubtitleNarrationDrain(queue);
        return;
      }
      const next = queue.pending[0];
      if (!next) {
        settleSubtitleNarrationDrain(queue);
        return;
      }
      queue.applying = true;
      const batchId = batchIdRef.current ?? newNarrationBatchId('narration');
      try {
        const target = queue.targets?.get(next.subtitleId) ?? narrationTargetById.get(next.subtitleId);
        await onApplyNarrationSegmentsRef.current(
          [buildNarrationSegment({
            ...next,
            sourceSubtitleId: next.sourceSubtitleId ?? target?.sourceSubtitleId,
            replaceSubtitleIds: next.replaceSubtitleIds ?? target?.replaceSubtitleIds,
            sourceSubtitleTimings: next.sourceSubtitleTimings ?? target?.sourceSubtitleTimings,
          }, batchId, profileRef.current)],
          next.replaceSubtitleIds ?? target?.replaceSubtitleIds ?? [target?.sourceSubtitleId ?? next.subtitleId],
        );
        if (queue.pending[0] === next) queue.pending.shift();
        queue.appliedIds.add(next.subtitleId);
      } catch (error) {
        const failure = error instanceof Error ? error : new Error(String(error));
        queue.failure = failure;
        if (isCurrentProject()) {
          const message = failure.message;
          notifySubtitleNarrationQueue(queue);
          publishStatus({
            state: 'error', message, error: message, progress: 0,
            totalItems: 0, completedItems: 0, activeSubtitleId: null, results: [], errors: [],
          }, { immediate: true });
        }
      } finally {
        queue.applying = false;
      }
      if (queue.failure) {
        settleSubtitleNarrationDrain(queue);
      } else if (queue.pending.length > 0) {
        scheduleApplyDrain();
      } else {
        settleSubtitleNarrationDrain(queue);
      }
    }, APPLY_RESULT_STREAM_INTERVAL_MS);
  }, [isCurrentProject, narrationTargetById, publishStatus]);

  const waitForApplyDrain = useCallback(() => {
    const queue = applyQueueRef.current;
    if (queue.failure) return Promise.reject(queue.failure);
    if (queue.pending.length === 0 && !queue.applying) {
      return Promise.resolve();
    }
    return new Promise<void>((resolve, reject) => {
      queue.waiters.push({ resolve, reject });
      scheduleApplyDrain();
    });
  }, [scheduleApplyDrain]);

  const applyInitialClear = useCallback(async (replaceSubtitleIds: string[]) => {
    await onApplyNarrationSegmentsRef.current(
      [],
      replaceSubtitleIds,
    );
  }, []);

  const queueApplyResults = useCallback((results: SubtitleNarrationResultItem[]) => {
    if (results.length === 0) return;
    const queue = applyQueueRef.current;
    const known = new Set([...queue.appliedIds, ...queue.pending.map((result) => result.subtitleId)]);
    for (const result of results) {
      if (known.has(result.subtitleId)) continue;
      const target = queue.targets?.get(result.subtitleId) ?? narrationTargetById.get(result.subtitleId);
      queue.pending.push({ ...result, sourceSubtitleId: target?.sourceSubtitleId,
        replaceSubtitleIds: target?.replaceSubtitleIds, sourceSubtitleTimings: target?.sourceSubtitleTimings });
      known.add(result.subtitleId);
    }
    scheduleApplyDrain();
  }, [narrationTargetById, scheduleApplyDrain]);

  useEffect(() => {
    if (jobId && isCurrentProject()) scheduleApplyDrain();
  }, [isCurrentProject, jobId, scheduleApplyDrain]);

  const reportApplyFailure = useCallback((error: unknown) => {
    if (!isCurrentProject()) return;
    const message = error instanceof Error ? error.message : String(error);
    const queue = applyQueueRef.current;
    queue.failure = error instanceof Error ? error : new Error(String(error));
    notifySubtitleNarrationQueue(queue);
    publishStatus({
      state: 'error', message, error: message, progress: 0,
      totalItems: 0, completedItems: 0, activeSubtitleId: null, results: [], errors: [],
    }, { immediate: true });
  }, [isCurrentProject, publishStatus]);

  const isOwnedRun = useCallback((id: string | null, queue: SubtitleNarrationApplyQueue) => {
    if (!id || !isCurrentProject() || queue.retired) return false;
    const saved = readResumableRun<ResumableSubtitleNarrationRun>(runNamespace);
    return saved?.jobId === id && saved.applyQueue === queue;
  }, [isCurrentProject, runNamespace]);

  const canPublishCompletion = useCallback((queue: SubtitleNarrationApplyQueue) => (
    isCurrentProject() && queue.retired && applyQueueRef.current === queue
    && !readResumableRun(runNamespace) && !hasResumableStart(runNamespace)
  ), [isCurrentProject, runNamespace]);

  const finishNarration = useCallback(async (id: string | null, queue: SubtitleNarrationApplyQueue, terminalStatus?: SubtitleNarrationJobStatus, cancelNative = false) => {
    if (!isOwnedRun(id, queue) || !id) return false;
    if (!queue.completion) {
      queue.terminalStatus = terminalStatus ?? null;
      queue.completion = (async () => {
        await waitForApplyDrain();
        if (!isOwnedRun(id, queue)) return;
        await finalizeNarration();
        if (isOwnedRun(id, queue)) {
          if (cancelNative) retireNarrationJob('cancel_subtitle_narration', id);
          else acknowledgeNarrationStart('cancel_subtitle_narration', id);
          queue.retired = true;
          retireNarrationBatch(batchIdRef.current);
          clearResumableRunIfJob(runNamespace, id);
        }
      })().catch((error: unknown) => {
        queue.completion = null;
        throw error;
      });
    }
    try {
      await queue.completion;
      if (isCurrentProject() && queue.retired) setJobId((current) => current === id ? null : current);
      return queue.retired;
    } catch (error) {
      reportApplyFailure(error);
      return false;
    }
  }, [finalizeNarration, isCurrentProject, isOwnedRun, reportApplyFailure, runNamespace, waitForApplyDrain]);

  useAsyncJobPoll<SubtitleNarrationJobStatus>({
    jobId,
    restartKey: pollGeneration,
    fetchStatus: (activeJobId) =>
      invoke<SubtitleNarrationJobStatus>('get_subtitle_narration_status', {
        jobId: activeJobId,
        knownResultsRevision: knownResultsRevisionRef.current,
        knownErrorCount: allErrorItemsRef.current.length,
      }),
    isTerminal: (nextStatus) =>
      nextStatus.state === 'completed'
      || nextStatus.state === 'cancelled'
      || nextStatus.state === 'error',
    onTick: (nextStatus) => {
      if (!isOwnedRun(jobId, currentApplyQueue)) return;
      knownResultsRevisionRef.current = Math.max(
        knownResultsRevisionRef.current,
        nextStatus.resultsRevision ?? knownResultsRevisionRef.current,
      );
      if (nextStatus.results.length > 0) {
        allResultItemsRef.current = [
          ...allResultItemsRef.current,
          ...nextStatus.results,
        ];
      }
      if (nextStatus.errors.length > 0) {
        allErrorItemsRef.current = [
          ...allErrorItemsRef.current,
          ...nextStatus.errors,
        ];
      }
      const publishedStatus: SubtitleNarrationJobStatus = {
        ...nextStatus,
        results: [],
        errors: allErrorItemsRef.current,
      };
      publishStatus(publishedStatus);

      queueApplyResults(nextStatus.results);
      if (jobId) {
        saveResumableRun<ResumableSubtitleNarrationRun>(runNamespace, {
          jobId,
          status: publishedStatus,
          batchId: batchIdRef.current,
          applyQueue: applyQueueRef.current,
          knownResultsRevision: knownResultsRevisionRef.current,
          allResultItems: allResultItemsRef.current,
          allErrorItems: allErrorItemsRef.current,
        });
      }
    },
    onComplete: async (nextStatus) => {
      if (!isCurrentProject()) return;
      if (nextStatus.state === 'completed') {
        const overlaps = countNarrationOverlaps(allResultItemsRef.current);
        const terminalStatus = {
          ...nextStatus,
          results: [],
          errors: allErrorItemsRef.current,
          message: overlaps > 0
            ? t.subtitleNarrationStatusCompleteWithOverlaps.replace('{count}', String(overlaps))
            : t.subtitleNarrationStatusComplete,
        };
        if (!(await finishNarration(jobId, currentApplyQueue, terminalStatus))) return;
        if (!canPublishCompletion(currentApplyQueue)) return;
        publishStatus(terminalStatus, { immediate: true });
        return;
      }
      // cancelled or error terminal state
      const terminalStatus = { ...nextStatus, results: [], errors: allErrorItemsRef.current };
      if (!(await finishNarration(jobId, currentApplyQueue, terminalStatus))) return;
      if (!canPublishCompletion(currentApplyQueue)) return;
      publishStatus(terminalStatus, { immediate: true });
    },
    onError: async (error) => {
      if (!isOwnedRun(jobId, currentApplyQueue)) return;
      const terminalStatus: SubtitleNarrationJobStatus = {
        state: 'error',
        message: error instanceof Error ? error.message : t.subtitleNarrationStatusFailed,
        progress: 0,
        totalItems: 0,
        completedItems: 0,
        activeSubtitleId: null,
        results: [],
        errors: [],
        error: error instanceof Error ? error.message : String(error),
      };
      publishStatus(terminalStatus, { immediate: true });
      await finishNarration(jobId, currentApplyQueue, terminalStatus, true);
    },
    intervalFor: (nextStatus) => (nextStatus.results.length > 0 ? 250 : 500),
    onCleanup: () => {
      if (pendingStatusTimerRef.current !== null) {
        window.clearTimeout(pendingStatusTimerRef.current);
        pendingStatusTimerRef.current = null;
      }
    },
  });

  const handleGenerateNarration = useCallback(async () => {
    if (jobId || isStarting || !isCurrentProject()) return;
    if (requestItems.length === 0) {
      setStatus({
        state: 'error',
        message: t.subtitleNarrationNoSource,
        progress: 0,
        totalItems: 0,
        completedItems: 0,
        activeSubtitleId: null,
        results: [],
        errors: [],
        error: t.subtitleNarrationNoSource,
      });
      return;
    }

    const startClaim = claimResumableStart(runNamespace);
    if (!startClaim) return;
    const batchId = newNarrationBatchId('narration');
    batchIdRef.current = batchId;
    applyQueueRef.current = createSubtitleNarrationApplyQueue();
    applyQueueRef.current.targets = new Map(narrationTargetById);
    knownResultsRevisionRef.current = 0;
    allResultItemsRef.current = [];
    allErrorItemsRef.current = [];
    let pendingStartId: string | null = null;
    try {
      pendingStartId = await recordNarrationStart('cancel_subtitle_narration');
      if (!isCurrentProject()) {
        acknowledgeNarrationStart('cancel_subtitle_narration', pendingStartId);
        pendingStartId = null;
        return;
      }
      attachResumableStartCancellation(runNamespace, startClaim, 'cancel_subtitle_narration', pendingStartId);
      await applyInitialClear(requestItems.flatMap((subtitle) => subtitle.replaceSubtitleIds));
      if (!isCurrentProject()) {
        retireNarrationJob('cancel_subtitle_narration', pendingStartId);
        pendingStartId = null;
        return;
      }
      const result = await invoke<{ jobId: string }>('start_subtitle_narration', {
        jobId: pendingStartId,
        items: requestItems,
        profile: profileRef.current,
        sourceLanguageCode: sourceLanguageCode?.trim() || null,
        grouping: {
          textBudgetUnits: groupTextBudget,
          vadSearchRadiusSec: NARRATION_GROUP_VAD_RADIUS_SEC,
        },
      });
      recordNarrationResponse('cancel_subtitle_narration', pendingStartId, result.jobId);
      if (!isCurrentProject()) {
        retireNarrationJob('cancel_subtitle_narration', result.jobId);
        pendingStartId = null;
        return;
      }
      const queuedStatus: SubtitleNarrationJobStatus = {
        state: 'queued',
        message: t.subtitleNarrationStatusStarting,
        progress: 0,
        totalItems: requestItems.length,
        completedItems: 0,
        activeSubtitleId: null,
        results: [],
        errors: [],
        error: null,
      };
      setStatus(queuedStatus);
      setJobId(result.jobId);
      saveResumableRun<ResumableSubtitleNarrationRun>(runNamespace, {
        jobId: result.jobId,
        status: queuedStatus,
        batchId: batchIdRef.current,
        applyQueue: applyQueueRef.current,
        knownResultsRevision: 0,
        allResultItems: [],
        allErrorItems: [],
      });
      if (projectId && batchIdRef.current) registerNarrationBatch(projectId, batchIdRef.current,
        () => retireNarrationJob('cancel_subtitle_narration', result.jobId));
      pendingStartId = null;
    } catch (error) {
      if (pendingStartId) retireNarrationJob('cancel_subtitle_narration', pendingStartId);
      if (!isCurrentProject()) return;
      setStatus({
        state: 'error',
        message: error instanceof Error ? error.message : t.subtitleNarrationStatusFailed,
        progress: 0,
        totalItems: 0,
        completedItems: 0,
        activeSubtitleId: null,
        results: [],
        errors: [],
        error: error instanceof Error ? error.message : String(error),
      });
    } finally {
      releaseResumableStart(runNamespace, startClaim);
    }
  }, [
    isStarting,
    isCurrentProject,
    runNamespace,
    jobId,
    applyInitialClear,
    requestItems,
    groupTextBudget,
    sourceLanguageCode,
    t.subtitleNarrationNoSource,
    t.subtitleNarrationStatusFailed,
    t.subtitleNarrationStatusStarting,
  ]);

  const handleCancelNarration = useCallback(
    buildCancelHandler({
      jobId,
      cancelCommand: 'cancel_subtitle_narration',
      onCancelled: async () => {
        if (!isOwnedRun(jobId, currentApplyQueue)) return;
        const saved = readResumableRun<ResumableSubtitleNarrationRun>(runNamespace);
        const terminalStatus = saved?.status ? { ...saved.status, state: 'cancelled' as const, message: t.subtitleNarrationStatusCancelled } : undefined;
        if (!(await finishNarration(jobId, currentApplyQueue, terminalStatus))) return;
        if (!canPublishCompletion(currentApplyQueue)) return;
        setStatus((prev) => prev ? {
          ...prev,
          state: 'cancelled',
          message: t.subtitleNarrationStatusCancelled,
        } : prev);
      },
    }),
    [canPublishCompletion, currentApplyQueue, finishNarration, isOwnedRun, jobId, runNamespace, t.subtitleNarrationStatusCancelled],
  );

  const handleRetryNarration = useCallback(async () => {
    if (!isCurrentProject()) return;
    applyQueueRef.current.failure = null;
    notifySubtitleNarrationQueue(applyQueueRef.current);
    try {
      await waitForApplyDrain();
      if (isCurrentProject()) setPollGeneration((value) => value + 1);
    } catch (error) {
      reportApplyFailure(error);
    }
  }, [isCurrentProject, reportApplyFailure, waitForApplyDrain]);

  return {
    canGenerateNarration: projectId !== null && requestItems.length > 0 && !jobId && !isStarting,
    isGeneratingNarration: Boolean(jobId) || isStarting,
    narrationTargetCount: requestItems.length,
    narrationGroupCount: buildNarrationGroupPreview(requestItems, groupTextBudget).groupCount,
    narrationGroupPreview,
    narrationStatus: status,
    applyRetryable,
    handleRetryNarration,
    handleGenerateNarration,
    handleCancelNarration,
  };
}
