import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { Translations } from '@/i18n';
import { invoke } from '@/lib/ipc';
import { overlapsRange, type TrackSelectionRange } from '@/lib/timelineSegmentSelection';
import type { NarrationSegment, SubtitleSegment, TtsProfileSnapshot } from '@/types/video';
import type { NarrationProfilePayload } from '@/hooks/useNarrationSettings';

interface SubtitleNarrationRequestItem {
  id: string;
  text: string;
  startTime: number;
  endTime: number;
  sourceSubtitleId: string;
  replaceSubtitleIds: string[];
}

interface SubtitleNarrationResultItem {
  subtitleId: string;
  text: string;
  path: string;
  duration: number;
  startTime: number;
  endTime: number;
  sourceSubtitleId?: string;
  replaceSubtitleIds?: string[];
}

interface SubtitleNarrationJobStatus {
  state: 'queued' | 'running' | 'completed' | 'cancelled' | 'error';
  message: string;
  progress: number;
  totalItems: number;
  completedItems: number;
  activeSubtitleId?: string | null;
  resultsRevision?: number;
  results: SubtitleNarrationResultItem[];
  errors: Array<{ subtitleId: string; message: string }>;
  error?: string | null;
}

const APPLY_RESULT_STREAM_INTERVAL_MS = 140;
const STATUS_UI_UPDATE_INTERVAL_MS = 900;

interface UseSubtitleNarrationParams {
  t: Translations;
  visibleSubtitles: SubtitleSegment[];
  selectedSubtitleIds?: string[];
  selectedSubtitleRange?: TrackSelectionRange | null;
  sourceLanguageCode?: string | null;
  profile: NarrationProfilePayload;
  readUnsplitSubtitles?: boolean;
  onApplyNarrationSegments: (
    segments: NarrationSegment[],
    replaceSubtitleIds: string[],
  ) => void | Promise<void>;
  onFinalizeNarrationSegments: () => void | Promise<void>;
}

function profileToSnapshot(profile: NarrationProfilePayload): TtsProfileSnapshot {
  return {
    method: profile.method,
    geminiModel: profile.geminiModel,
    geminiVoice: profile.geminiVoice,
    geminiSpeed: profile.geminiSpeed,
    geminiInstruction: profile.geminiInstruction,
    googleSpeed: profile.googleSpeed,
    edgeVoice: profile.edgeVoice,
    edgePitch: profile.edgePitch,
    edgeRate: profile.edgeRate,
    edgeVoiceConfigs: profile.edgeVoiceConfigs,
    stepAudioVoice: profile.stepAudioVoice,
    stepAudioReferenceVoiceId: profile.stepAudioReferenceVoiceId,
    stepAudioPromptText: profile.stepAudioPromptText,
    stepAudioUseCustomReference: profile.stepAudioUseCustomReference,
    stepAudioReferenceAudioPath: profile.stepAudioReferenceAudioPath,
    stepAudioReferenceText: profile.stepAudioReferenceText,
    stepAudioReferenceLabel: profile.stepAudioReferenceLabel,
    magpieVoice: profile.magpieVoice,
    magpieVoiceConfigs: profile.magpieVoiceConfigs,
    kokoroVoice: profile.kokoroVoice,
    kokoroSpeed: profile.kokoroSpeed,
    kokoroNumThreads: profile.kokoroNumThreads,
    kokoroVoiceConfigs: profile.kokoroVoiceConfigs,
    supertonicSpeed: profile.supertonicSpeed,
    supertonicNumSteps: profile.supertonicNumSteps,
    supertonicNumThreads: profile.supertonicNumThreads,
    supertonicVoiceConfigs: profile.supertonicVoiceConfigs,
  };
}

function buildNarrationSegment(
  result: SubtitleNarrationResultItem,
  batchId: string,
  profile: NarrationProfilePayload,
): NarrationSegment {
  const duration = Math.max(0.05, result.duration);
  return {
    id: `${batchId}-${result.subtitleId}`,
    rawAudioPath: result.path,
    name: result.text.trim().slice(0, 42) || 'Narration',
    duration,
    startTime: Math.max(0, result.startTime),
    inPoint: 0,
    outPoint: duration,
    playbackRate: 1,
    addedAt: Date.now(),
    sourceSubtitleId: result.sourceSubtitleId ?? result.subtitleId,
    sourceSubtitleIds: result.replaceSubtitleIds,
    narrationBatchId: batchId,
    ttsProfileSnapshot: profileToSnapshot(profile),
  };
}

function countNarrationOverlaps(results: readonly SubtitleNarrationResultItem[]) {
  const sorted = [...results].sort((a, b) => a.startTime - b.startTime);
  let count = 0;
  for (let index = 0; index < sorted.length - 1; index += 1) {
    const current = sorted[index];
    const next = sorted[index + 1];
    if (current.startTime + current.duration > next.startTime + 0.05) {
      count += 1;
    }
  }
  return count;
}

export function useSubtitleNarration({
  t,
  visibleSubtitles,
  selectedSubtitleIds,
  selectedSubtitleRange,
  sourceLanguageCode,
  profile,
  readUnsplitSubtitles = true,
  onApplyNarrationSegments,
  onFinalizeNarrationSegments,
}: UseSubtitleNarrationParams) {
  const [jobId, setJobId] = useState<string | null>(null);
  const [status, setStatus] = useState<SubtitleNarrationJobStatus | null>(null);
  const [isStarting, setIsStarting] = useState(false);
  const batchIdRef = useRef<string | null>(null);
  const appliedResultIdsRef = useRef<Set<string>>(new Set());
  const knownResultsRevisionRef = useRef(0);
  const allResultItemsRef = useRef<SubtitleNarrationResultItem[]>([]);
  const allErrorItemsRef = useRef<Array<{ subtitleId: string; message: string }>>([]);
  const pendingApplyResultsRef = useRef<SubtitleNarrationResultItem[]>([]);
  const pendingApplyTimerRef = useRef<number | null>(null);
  const pendingStatusTimerRef = useRef<number | null>(null);
  const lastStatusUiCommitAtRef = useRef(0);
  const latestStatusRef = useRef<SubtitleNarrationJobStatus | null>(null);
  const isApplyingResultRef = useRef(false);
  const drainResolversRef = useRef<Array<() => void>>([]);
  const profileRef = useRef<NarrationProfilePayload>(profile);
  const onApplyNarrationSegmentsRef = useRef(onApplyNarrationSegments);
  useEffect(() => {
    profileRef.current = profile;
  }, [profile]);
  useEffect(() => {
    onApplyNarrationSegmentsRef.current = onApplyNarrationSegments;
  }, [onApplyNarrationSegments]);

  const targetSubtitles = useMemo(() => {
    const selection = new Set(selectedSubtitleIds ?? []);
    const hasSelection = selection.size > 0;
    return visibleSubtitles
      .filter((subtitle) => {
        if (hasSelection) return selection.has(subtitle.id);
        if (selectedSubtitleRange) return overlapsRange(subtitle, selectedSubtitleRange);
        return true;
      })
      .sort((a, b) => a.startTime - b.startTime);
  }, [selectedSubtitleIds, selectedSubtitleRange, visibleSubtitles]);

  const narrationTargets = useMemo<SubtitleNarrationRequestItem[]>(() => {
    if (!readUnsplitSubtitles) {
      return targetSubtitles.map((subtitle) => ({
        id: subtitle.id,
        text: subtitle.text.trim(),
        startTime: subtitle.startTime,
        endTime: subtitle.endTime,
        sourceSubtitleId: subtitle.id,
        replaceSubtitleIds: [subtitle.id],
      }));
    }

    const groups = new Map<string, SubtitleSegment[]>();
    const ungrouped: SubtitleSegment[] = [];
    for (const subtitle of targetSubtitles) {
      if (subtitle.splitGroupId && subtitle.splitGroupText) {
        const group = groups.get(subtitle.splitGroupId) ?? [];
        group.push(subtitle);
        groups.set(subtitle.splitGroupId, group);
      } else {
        ungrouped.push(subtitle);
      }
    }

    const groupedTargets = [...groups.entries()].map(([groupId, subtitles]) => {
      const sorted = [...subtitles].sort((a, b) =>
        (a.splitGroupIndex ?? 0) - (b.splitGroupIndex ?? 0) ||
        a.startTime - b.startTime,
      );
      const first = sorted[0];
      return {
        id: groupId,
        text: (first?.splitGroupText ?? sorted.map((subtitle) => subtitle.text).join(' ')).trim(),
        startTime: first?.splitGroupStartTime ?? Math.min(...sorted.map((subtitle) => subtitle.startTime)),
        endTime: first?.splitGroupEndTime ?? Math.max(...sorted.map((subtitle) => subtitle.endTime)),
        sourceSubtitleId: first?.id ?? groupId,
        replaceSubtitleIds: sorted.map((subtitle) => subtitle.id),
      };
    });

    return [
      ...groupedTargets,
      ...ungrouped.map((subtitle) => ({
        id: subtitle.id,
        text: subtitle.text.trim(),
        startTime: subtitle.startTime,
        endTime: subtitle.endTime,
        sourceSubtitleId: subtitle.id,
        replaceSubtitleIds: [subtitle.id],
      })),
    ].sort((a, b) => a.startTime - b.startTime || a.id.localeCompare(b.id));
  }, [readUnsplitSubtitles, targetSubtitles]);

  const narrationTargetById = useMemo(
    () => new Map(narrationTargets.map((target) => [target.id, target])),
    [narrationTargets],
  );

  const requestItems = useMemo<SubtitleNarrationRequestItem[]>(() => (
    narrationTargets
      .filter((subtitle) => subtitle.text.length > 0)
  ), [narrationTargets]);

  const finalizeNarration = useCallback(async () => {
    await onFinalizeNarrationSegments();
  }, [onFinalizeNarrationSegments]);

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

  const resolveDrainWaiters = useCallback(() => {
    if (pendingApplyResultsRef.current.length > 0 || isApplyingResultRef.current) return;
    const resolvers = drainResolversRef.current;
    if (resolvers.length === 0) return;
    drainResolversRef.current = [];
    resolvers.forEach((resolve) => resolve());
  }, []);

  const scheduleApplyDrain = useCallback(() => {
    if (pendingApplyTimerRef.current !== null || isApplyingResultRef.current) return;
    pendingApplyTimerRef.current = window.setTimeout(async () => {
      pendingApplyTimerRef.current = null;
      const next = pendingApplyResultsRef.current.shift();
      if (!next) {
        resolveDrainWaiters();
        return;
      }
      isApplyingResultRef.current = true;
      const batchId = batchIdRef.current ?? `narration-${Date.now()}`;
      try {
        const target = narrationTargetById.get(next.subtitleId);
        await onApplyNarrationSegmentsRef.current(
          [buildNarrationSegment({
            ...next,
            sourceSubtitleId: target?.sourceSubtitleId,
            replaceSubtitleIds: target?.replaceSubtitleIds,
          }, batchId, profileRef.current)],
          target?.replaceSubtitleIds ?? [target?.sourceSubtitleId ?? next.subtitleId],
        );
      } finally {
        isApplyingResultRef.current = false;
      }
      if (pendingApplyResultsRef.current.length > 0) {
        scheduleApplyDrain();
      } else {
        resolveDrainWaiters();
      }
    }, APPLY_RESULT_STREAM_INTERVAL_MS);
  }, [narrationTargetById, resolveDrainWaiters]);

  const waitForApplyDrain = useCallback(() => {
    if (pendingApplyResultsRef.current.length === 0 && !isApplyingResultRef.current) {
      return Promise.resolve();
    }
    return new Promise<void>((resolve) => {
      drainResolversRef.current.push(resolve);
      scheduleApplyDrain();
    });
  }, [scheduleApplyDrain]);

  const flushPendingApplyResults = useCallback(async () => {
    if (pendingApplyTimerRef.current !== null) {
      window.clearTimeout(pendingApplyTimerRef.current);
      pendingApplyTimerRef.current = null;
    }
    while (pendingApplyResultsRef.current.length > 0) {
      const next = pendingApplyResultsRef.current.shift();
      if (!next) break;
      isApplyingResultRef.current = true;
      const batchId = batchIdRef.current ?? `narration-${Date.now()}`;
      try {
        const target = narrationTargetById.get(next.subtitleId);
        await onApplyNarrationSegmentsRef.current(
          [buildNarrationSegment({
            ...next,
            sourceSubtitleId: target?.sourceSubtitleId,
            replaceSubtitleIds: target?.replaceSubtitleIds,
          }, batchId, profileRef.current)],
          target?.replaceSubtitleIds ?? [target?.sourceSubtitleId ?? next.subtitleId],
        );
      } finally {
        isApplyingResultRef.current = false;
      }
    }
    resolveDrainWaiters();
  }, [narrationTargetById, resolveDrainWaiters]);

  const applyInitialClear = useCallback(async (replaceSubtitleIds: string[]) => {
    await onApplyNarrationSegmentsRef.current(
      [],
      replaceSubtitleIds,
    );
  }, []);

  const queueApplyResults = useCallback((results: SubtitleNarrationResultItem[]) => {
    if (results.length === 0) return;
    pendingApplyResultsRef.current = [
      ...pendingApplyResultsRef.current,
      ...results,
    ];
    scheduleApplyDrain();
  }, [scheduleApplyDrain]);

  useEffect(() => {
    if (!jobId) return;
    let cancelled = false;

    const poll = async () => {
      try {
        const pollStartedAt = performance.now();
        const nextStatus = await invoke<SubtitleNarrationJobStatus>(
          'get_subtitle_narration_status',
          {
            jobId,
            knownResultsRevision: knownResultsRevisionRef.current,
            knownErrorCount: allErrorItemsRef.current.length,
          },
        );
        const invokeMs = performance.now() - pollStartedAt;
        if (cancelled) return;
        knownResultsRevisionRef.current = Math.max(
          knownResultsRevisionRef.current,
          nextStatus.resultsRevision ?? knownResultsRevisionRef.current,
        );
        if (invokeMs > 80 || nextStatus.results.length > 0) {
          console.info(
            `[NarrationPerf][StatusPoll] job=${jobId} state=${nextStatus.state} results=${nextStatus.results.length} revision=${nextStatus.resultsRevision ?? 0} completed=${nextStatus.completedItems}/${nextStatus.totalItems} invoke_ms=${invokeMs.toFixed(1)}`,
          );
        }
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
        publishStatus({
          ...nextStatus,
          results: [],
          errors: allErrorItemsRef.current,
        });

        const newResults = nextStatus.results.filter((result) => {
          if (appliedResultIdsRef.current.has(result.subtitleId)) return false;
          appliedResultIdsRef.current.add(result.subtitleId);
          return true;
        });
        queueApplyResults(newResults);

        if (nextStatus.state === 'completed') {
          await waitForApplyDrain();
          const overlaps = countNarrationOverlaps(allResultItemsRef.current);
          publishStatus({
            ...nextStatus,
            results: [],
            errors: allErrorItemsRef.current,
            message: overlaps > 0
              ? t.subtitleNarrationStatusCompleteWithOverlaps.replace('{count}', String(overlaps))
              : t.subtitleNarrationStatusComplete,
          }, { immediate: true });
          setJobId(null);
          await finalizeNarration();
          return;
        }
        if (nextStatus.state === 'cancelled' || nextStatus.state === 'error') {
          setJobId(null);
          await waitForApplyDrain();
          publishStatus({
            ...nextStatus,
            results: [],
            errors: allErrorItemsRef.current,
          }, { immediate: true });
          await finalizeNarration();
          return;
        }
        window.setTimeout(poll, nextStatus.results.length > 0 ? 250 : 500);
      } catch (error) {
        if (cancelled) return;
        publishStatus({
          state: 'error',
          message: error instanceof Error ? error.message : t.subtitleNarrationStatusFailed,
          progress: 0,
          totalItems: 0,
          completedItems: 0,
          activeSubtitleId: null,
          results: [],
          errors: [],
          error: error instanceof Error ? error.message : String(error),
        }, { immediate: true });
        setJobId(null);
        await flushPendingApplyResults();
        await finalizeNarration();
      }
    };

    void poll();
    return () => {
      cancelled = true;
      if (pendingApplyTimerRef.current !== null) {
        window.clearTimeout(pendingApplyTimerRef.current);
        pendingApplyTimerRef.current = null;
      }
      if (pendingStatusTimerRef.current !== null) {
        window.clearTimeout(pendingStatusTimerRef.current);
        pendingStatusTimerRef.current = null;
      }
    };
  }, [
    finalizeNarration,
    flushPendingApplyResults,
    jobId,
    publishStatus,
    queueApplyResults,
    waitForApplyDrain,
    t.subtitleNarrationStatusComplete,
    t.subtitleNarrationStatusCompleteWithOverlaps,
    t.subtitleNarrationStatusFailed,
  ]);

  const handleGenerateNarration = useCallback(async () => {
    if (jobId || isStarting) return;
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

    const batchId = `narration-${Date.now()}`;
    batchIdRef.current = batchId;
    appliedResultIdsRef.current = new Set();
    knownResultsRevisionRef.current = 0;
    allResultItemsRef.current = [];
    allErrorItemsRef.current = [];
    setIsStarting(true);
    try {
      pendingApplyResultsRef.current = [];
      await applyInitialClear(requestItems.flatMap((subtitle) => subtitle.replaceSubtitleIds));
      const result = await invoke<{ jobId: string }>('start_subtitle_narration', {
        items: requestItems,
        profile: profileRef.current,
        sourceLanguageCode: sourceLanguageCode?.trim() || null,
      });
      setStatus({
        state: 'queued',
        message: t.subtitleNarrationStatusStarting,
        progress: 0,
        totalItems: requestItems.length,
        completedItems: 0,
        activeSubtitleId: null,
        results: [],
        errors: [],
        error: null,
      });
      setJobId(result.jobId);
    } catch (error) {
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
      setIsStarting(false);
    }
  }, [
    isStarting,
    jobId,
    applyInitialClear,
    requestItems,
    sourceLanguageCode,
    t.subtitleNarrationNoSource,
    t.subtitleNarrationStatusFailed,
    t.subtitleNarrationStatusStarting,
  ]);

  const handleCancelNarration = useCallback(async () => {
    if (!jobId) return;
    await invoke('cancel_subtitle_narration', { jobId });
    setStatus((prev) => prev ? {
      ...prev,
      state: 'cancelled',
      message: t.subtitleNarrationStatusCancelled,
    } : prev);
    setJobId(null);
    await finalizeNarration();
  }, [finalizeNarration, jobId, t.subtitleNarrationStatusCancelled]);

  return {
    canGenerateNarration: requestItems.length > 0 && !jobId && !isStarting,
    isGeneratingNarration: Boolean(jobId) || isStarting,
    narrationTargetCount: requestItems.length,
    narrationStatus: status,
    handleGenerateNarration,
    handleCancelNarration,
  };
}
