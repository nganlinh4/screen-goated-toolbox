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
}

interface SubtitleNarrationResultItem {
  subtitleId: string;
  text: string;
  path: string;
  duration: number;
  startTime: number;
  endTime: number;
}

interface SubtitleNarrationJobStatus {
  state: 'queued' | 'running' | 'completed' | 'cancelled' | 'error';
  message: string;
  progress: number;
  totalItems: number;
  completedItems: number;
  activeSubtitleId?: string | null;
  results: SubtitleNarrationResultItem[];
  errors: Array<{ subtitleId: string; message: string }>;
  error?: string | null;
}

interface UseSubtitleNarrationParams {
  t: Translations;
  visibleSubtitles: SubtitleSegment[];
  selectedSubtitleIds?: string[];
  selectedSubtitleRange?: TrackSelectionRange | null;
  profile: NarrationProfilePayload;
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
    sourceSubtitleId: result.subtitleId,
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
  profile,
  onApplyNarrationSegments,
  onFinalizeNarrationSegments,
}: UseSubtitleNarrationParams) {
  const [jobId, setJobId] = useState<string | null>(null);
  const [status, setStatus] = useState<SubtitleNarrationJobStatus | null>(null);
  const [isStarting, setIsStarting] = useState(false);
  const batchIdRef = useRef<string | null>(null);
  const appliedResultIdsRef = useRef<Set<string>>(new Set());
  const profileRef = useRef<NarrationProfilePayload>(profile);
  useEffect(() => {
    profileRef.current = profile;
  }, [profile]);

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

  const requestItems = useMemo<SubtitleNarrationRequestItem[]>(() => (
    targetSubtitles
      .map((subtitle) => ({
        id: subtitle.id,
        text: subtitle.text.trim(),
        startTime: subtitle.startTime,
        endTime: subtitle.endTime,
      }))
      .filter((subtitle) => subtitle.text.length > 0)
  ), [targetSubtitles]);

  const finalizeNarration = useCallback(async () => {
    await onFinalizeNarrationSegments();
  }, [onFinalizeNarrationSegments]);

  useEffect(() => {
    if (!jobId) return;
    let cancelled = false;

    const poll = async () => {
      try {
        const nextStatus = await invoke<SubtitleNarrationJobStatus>(
          'get_subtitle_narration_status',
          { jobId },
        );
        if (cancelled) return;
        setStatus(nextStatus);

        const batchId = batchIdRef.current ?? jobId;
        const newResults = nextStatus.results.filter((result) => {
          if (appliedResultIdsRef.current.has(result.subtitleId)) return false;
          appliedResultIdsRef.current.add(result.subtitleId);
          return true;
        });
        if (newResults.length > 0) {
          await onApplyNarrationSegments(
            newResults.map((result) => buildNarrationSegment(result, batchId, profileRef.current)),
            newResults.map((result) => result.subtitleId),
          );
        }

        if (nextStatus.state === 'completed') {
          const overlaps = countNarrationOverlaps(nextStatus.results);
          setStatus({
            ...nextStatus,
            message: overlaps > 0
              ? t.subtitleNarrationStatusCompleteWithOverlaps.replace('{count}', String(overlaps))
              : t.subtitleNarrationStatusComplete,
          });
          setJobId(null);
          await finalizeNarration();
          return;
        }
        if (nextStatus.state === 'cancelled' || nextStatus.state === 'error') {
          setJobId(null);
          await finalizeNarration();
          return;
        }
        window.setTimeout(poll, nextStatus.results.length > 0 ? 250 : 500);
      } catch (error) {
        if (cancelled) return;
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
        setJobId(null);
        await finalizeNarration();
      }
    };

    void poll();
    return () => {
      cancelled = true;
    };
  }, [
    finalizeNarration,
    jobId,
    onApplyNarrationSegments,
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
    setIsStarting(true);
    try {
      await onApplyNarrationSegments([], targetSubtitles.map((subtitle) => subtitle.id));
      const result = await invoke<{ jobId: string }>('start_subtitle_narration', {
        items: requestItems,
        profile: profileRef.current,
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
    onApplyNarrationSegments,
    requestItems,
    t.subtitleNarrationNoSource,
    t.subtitleNarrationStatusFailed,
    t.subtitleNarrationStatusStarting,
    targetSubtitles,
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
