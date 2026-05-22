import { useCallback, useEffect, useRef, useState } from 'react';
import type { Translations } from '@/i18n';
import { invoke, logToHost } from '@/lib/ipc';
import {
  buildSubtitleGenerationPlan,
  type SubtitleGenerationPlan,
  type SubtitleSource,
} from '@/lib/subtitleGenerationPlan';
import { defaultSubtitleStyle } from '@/lib/subtitleDefaults';
import {
  ORIGINAL_SUBTITLE_TRACK_ID,
  getTranslationSubtitleTrackId,
  normalizeSubtitleTrackState,
  setActiveSubtitleTrackView,
  upsertSubtitleTrack,
} from '@/lib/subtitleTracks';
import type {
  NarrationSegment,
  ProjectComposition,
  SubtitleSegment,
  SubtitleTrack,
  VideoSegment,
} from '@/types/video';
import type { TrackSelectionRange } from '@/lib/timelineSegmentSelection';

interface S2sNarrationSegmentResult {
  id: string;
  clipId: string;
  sourceText: string;
  targetText: string;
  startTime: number;
  endTime: number;
  path: string;
  duration: number;
}

interface S2sNarrationClipResult {
  clipId: string;
  isPartial: boolean;
  segments: S2sNarrationSegmentResult[];
}

interface S2sNarrationStatus {
  state: 'queued' | 'running' | 'completed' | 'cancelled' | 'error';
  message: string;
  progress: number;
  totalClips: number;
  completedClips: number;
  activeClipId?: string | null;
  vadSegmentDone?: number;
  vadSegmentTotal?: number;
  vadNoSpeech?: boolean;
  resultsRevision: number;
  results: S2sNarrationClipResult[];
  error?: string | null;
}

interface UseS2sNarrationParams {
  t: Translations;
  segment: VideoSegment | null;
  composition: ProjectComposition | null;
  activeClipId?: string | null;
  currentRawVideoPath: string;
  currentRawMicAudioPath: string;
  duration: number;
  sourceType: SubtitleSource;
  selectedRange?: TrackSelectionRange | null;
  targetLanguage: string;
  geminiModel: string;
  geminiVoice: string;
  geminiSpeed: string;
  parallelRequests: number;
  groupTextBudget: number;
  onApplyNarrationSegments: (
    segments: NarrationSegment[],
    replaceSubtitleIds: string[],
  ) => void | Promise<void>;
  onPopulateEmptySubtitles: (
    sourceSegments: SubtitleSegment[],
    targetSegments: SubtitleSegment[],
    targetLanguage: string,
  ) => void;
  onFinalize: () => void | Promise<void>;
}

function mappedTime(
  result: S2sNarrationSegmentResult,
  plan: SubtitleGenerationPlan,
  field: 'startTime' | 'endTime',
) {
  const transform = plan.clipTransformsByClip[result.clipId];
  if (!transform) return result[field];
  return result[field] + transform.timelineOffsetSec;
}

function s2sSubtitleId(result: S2sNarrationSegmentResult, kind: 'source' | 'target') {
  return `${result.id}-${kind}`;
}

function buildSubtitle(
  result: S2sNarrationSegmentResult,
  plan: SubtitleGenerationPlan,
  kind: 'source' | 'target',
  text: string,
): SubtitleSegment {
  const transform = plan.clipTransformsByClip[result.clipId];
  const startTime = mappedTime(result, plan, 'startTime');
  const endTime = Math.max(startTime + 0.05, mappedTime(result, plan, 'endTime'));
  const base: SubtitleSegment = {
    id: s2sSubtitleId(result, kind),
    startTime,
    endTime,
    text,
    style: defaultSubtitleStyle(),
    sourceGroup: {
      kind: transform ? 'audio' : plan.sourceTypeForNative,
      assignment: 'generated',
      audioSegmentId: transform?.audioSegmentId,
      sourceName: transform?.sourceName,
      sourcePath: transform?.sourcePath,
    },
  };
  if (!transform) return base;
  return {
    ...base,
    provenance: {
      sourceKind: 'audio',
      audioSegmentId: transform.audioSegmentId,
      sourceName: transform.sourceName,
      sourcePath: transform.sourcePath,
      sourceLocalStartTime: Math.max(0, result.startTime - transform.sourceLocalOffsetSec),
      sourceLocalEndTime: Math.max(0, result.endTime - transform.sourceLocalOffsetSec),
    },
  };
}

function buildNarration(
  result: S2sNarrationSegmentResult,
  plan: SubtitleGenerationPlan,
  batchId: string,
): NarrationSegment {
  const startTime = mappedTime(result, plan, 'startTime');
  const name = result.targetText.trim() || result.sourceText.trim() || 'Gemini S2S';
  const targetSubtitleId = s2sSubtitleId(result, 'target');
  return {
    id: `${batchId}-${result.id}`,
    rawAudioPath: result.path,
    name: name.slice(0, 42),
    duration: Math.max(0.05, result.duration),
    startTime,
    inPoint: 0,
    outPoint: Math.max(0.05, result.duration),
    playbackRate: 1,
    addedAt: Date.now(),
    sourceSubtitleId: targetSubtitleId,
    sourceSubtitleIds: [targetSubtitleId],
    narrationBatchId: batchId,
    narrationAlignmentMode: 'single',
    narrationAlignmentConfidence: 1,
    ttsProfileSnapshot: {
      method: 'gemini-live-s2s',
    },
  };
}

function formatTemplate(template: string, values: Record<string, string | number>) {
  return Object.entries(values).reduce(
    (text, [key, value]) => text.split(`{${key}}`).join(String(value)),
    template,
  );
}

function localizeS2sStatus(t: Translations, status: S2sNarrationStatus) {
  const totalClips = Math.max(1, status.totalClips || 1);
  const activeClip = Math.min(totalClips, Math.max(1, status.completedClips + 1));
  if (status.state === 'queued') return t.narrationS2sQueued;
  if (status.state === 'completed') return t.narrationS2sComplete;
  if (status.state === 'cancelled') return t.subtitleNarrationStatusCancelled;
  if (status.state === 'error') return status.error || status.message || t.narrationS2sFailed;
  if (status.vadNoSpeech) {
    return formatTemplate(t.narrationS2sNoSpeech, {
      clip: activeClip,
      clips: totalClips,
    });
  }
  if ((status.vadSegmentTotal ?? 0) > 0) {
    return formatTemplate(t.narrationS2sVadProgress, {
      clip: activeClip,
      clips: totalClips,
      done: status.vadSegmentDone ?? 0,
      total: status.vadSegmentTotal ?? 0,
    });
  }
  return status.state === 'running'
    ? formatTemplate(t.narrationS2sRunning, { clip: activeClip, clips: totalClips })
    : t.narrationS2sStarting;
}

function mergeS2sSubtitleSegments(
  existingSegments: readonly SubtitleSegment[],
  incomingSegments: readonly SubtitleSegment[],
) {
  if (incomingSegments.length === 0) return [...existingSegments];
  const incomingIds = new Set(incomingSegments.map((segment) => segment.id));
  const rangeStart = Math.min(...incomingSegments.map((segment) => segment.startTime));
  const rangeEnd = Math.max(...incomingSegments.map((segment) => segment.endTime));
  const epsilon = 0.001;
  const kept = existingSegments.filter((segment) => {
    if (incomingIds.has(segment.id)) return false;
    const outsideRange =
      segment.endTime <= rangeStart + epsilon ||
      segment.startTime >= rangeEnd - epsilon;
    return outsideRange;
  });
  return [...kept, ...incomingSegments].sort((left, right) =>
    left.startTime === right.startTime
      ? left.endTime - right.endTime
      : left.startTime - right.startTime,
  );
}

export function useS2sNarration({
  t,
  segment,
  composition,
  activeClipId,
  currentRawVideoPath,
  currentRawMicAudioPath,
  duration,
  sourceType,
  selectedRange,
  targetLanguage,
  geminiModel,
  geminiVoice,
  geminiSpeed,
  parallelRequests,
  groupTextBudget,
  onApplyNarrationSegments,
  onPopulateEmptySubtitles,
  onFinalize,
}: UseS2sNarrationParams) {
  const [jobId, setJobId] = useState<string | null>(null);
  const [status, setStatus] = useState<S2sNarrationStatus | null>(null);
  const [isStarting, setIsStarting] = useState(false);
  const revisionRef = useRef(0);
  const planRef = useRef<SubtitleGenerationPlan | null>(null);
  const batchIdRef = useRef<string>('');
  const sourceSubtitleResultsRef = useRef<SubtitleSegment[]>([]);
  const targetSubtitleResultsRef = useRef<SubtitleSegment[]>([]);
  const lastProgressSignatureRef = useRef('');
  const lastProgressAtRef = useRef(0);
  const lastStallLogAtRef = useRef(0);

  useEffect(() => {
    if (!jobId) return;
    let cancelled = false;
    const poll = async () => {
      try {
        const next = await invoke<S2sNarrationStatus>('get_s2s_narration_status', {
          jobId,
          knownResultsRevision: revisionRef.current,
        });
        if (cancelled) return;
        const progressSignature = [
          next.state,
          next.completedClips,
          next.activeClipId ?? '',
          next.vadSegmentDone ?? 0,
          next.vadSegmentTotal ?? 0,
          next.resultsRevision,
        ].join(':');
        const now = performance.now();
        if (progressSignature !== lastProgressSignatureRef.current) {
          lastProgressSignatureRef.current = progressSignature;
          lastProgressAtRef.current = now;
        } else if (
          next.state === 'running' &&
          now - lastProgressAtRef.current > 15_000 &&
          now - lastStallLogAtRef.current > 15_000
        ) {
          lastStallLogAtRef.current = now;
          logToHost(
            `[S2SNarration][FrontendStall] job=${jobId} state=${next.state} completed=${next.completedClips}/${next.totalClips} active=${next.activeClipId ?? ''} vad=${next.vadSegmentDone ?? 0}/${next.vadSegmentTotal ?? 0} revision=${next.resultsRevision} message=${next.message}`,
          );
        }
        revisionRef.current = Math.max(revisionRef.current, next.resultsRevision);
        setStatus({ ...next, message: localizeS2sStatus(t, next), results: [] });
        const plan = planRef.current;
        if (plan && next.results.length > 0) {
          const flat = next.results.flatMap((result) => result.segments);
          const narrations = flat.map((result) =>
            buildNarration(result, plan, batchIdRef.current),
          );
          const replaceIds = flat.map((result) => s2sSubtitleId(result, 'target'));
          sourceSubtitleResultsRef.current.push(
            ...flat.map((result) =>
              buildSubtitle(result, plan, 'source', result.sourceText),
            ),
          );
          targetSubtitleResultsRef.current.push(
            ...flat.map((result) =>
              buildSubtitle(result, plan, 'target', result.targetText),
            ),
          );
          onPopulateEmptySubtitles(
            sourceSubtitleResultsRef.current,
            targetSubtitleResultsRef.current,
            targetLanguage,
          );
          await onApplyNarrationSegments(narrations, replaceIds);
        }
        if (next.state === 'completed') {
          onPopulateEmptySubtitles(
            sourceSubtitleResultsRef.current,
            targetSubtitleResultsRef.current,
            targetLanguage,
          );
          setJobId(null);
          await onFinalize();
          return;
        }
        if (next.state === 'cancelled' || next.state === 'error') {
          setJobId(null);
          await onFinalize();
          return;
        }
        window.setTimeout(poll, next.results.length > 0 ? 250 : 600);
      } catch (error) {
        if (cancelled) return;
        setStatus({
          state: 'error',
          message: error instanceof Error ? error.message : String(error),
          progress: 0,
          totalClips: 0,
          completedClips: 0,
          resultsRevision: revisionRef.current,
          results: [],
          error: error instanceof Error ? error.message : String(error),
        });
        setJobId(null);
        await onFinalize();
      }
    };
    void poll();
    return () => {
      cancelled = true;
    };
  }, [
    jobId,
    onApplyNarrationSegments,
    onFinalize,
    onPopulateEmptySubtitles,
    t,
    targetLanguage,
  ]);

  const handleGenerate = useCallback(async () => {
    if (jobId || isStarting) return;
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
      setStatus({
        state: 'error',
        message: t.subtitleStatusNoSource,
        progress: 0,
        totalClips: 0,
        completedClips: 0,
        resultsRevision: 0,
        results: [],
        error: t.subtitleStatusNoSource,
      });
      return;
    }
    setIsStarting(true);
    try {
      planRef.current = plan;
      batchIdRef.current = `s2s-narration-${Date.now()}`;
      sourceSubtitleResultsRef.current = [];
      targetSubtitleResultsRef.current = [];
      revisionRef.current = 0;
      lastProgressSignatureRef.current = '';
      lastProgressAtRef.current = performance.now();
      lastStallLogAtRef.current = 0;
      const response = await invoke<{ jobId: string }>('start_s2s_narration', {
        sourceType: plan.sourceTypeForNative,
        targetLanguage,
        geminiModel,
        geminiVoice,
        geminiSpeed,
        parallelRequests,
        groupTextBudget,
        clips: plan.clips,
      });
      setStatus({
        state: 'queued',
        message: t.narrationS2sQueued,
        progress: 0,
        totalClips: plan.clips.length,
        completedClips: 0,
        resultsRevision: 0,
        results: [],
        error: null,
      });
      setJobId(response.jobId);
    } catch (error) {
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
      setIsStarting(false);
    }
  }, [
    activeClipId,
    composition,
    currentRawMicAudioPath,
    currentRawVideoPath,
    duration,
    geminiModel,
    geminiSpeed,
    geminiVoice,
    groupTextBudget,
    parallelRequests,
    isStarting,
    jobId,
    segment,
    selectedRange,
    sourceType,
    t.narrationS2sQueued,
    t.subtitleStatusNoSource,
    targetLanguage,
  ]);

  const handleCancel = useCallback(async () => {
    if (!jobId) return;
    await invoke('cancel_s2s_narration', { jobId });
    setJobId(null);
    setStatus((prev) => prev ? { ...prev, state: 'cancelled', message: 'Cancelled' } : prev);
    await onFinalize();
  }, [jobId, onFinalize]);

  return {
    canGenerate: !jobId && !isStarting,
    isGenerating: Boolean(jobId) || isStarting,
    status,
    handleGenerate,
    handleCancel,
  };
}

export function populateEmptyS2sSubtitleTracks(
  segment: VideoSegment,
  sourceSegments: SubtitleSegment[],
  targetSegments: SubtitleSegment[],
  targetLanguage: string,
): VideoSegment {
  if (sourceSegments.length === 0 && targetSegments.length === 0) {
    return normalizeSubtitleTrackState(segment);
  }
  const normalized = normalizeSubtitleTrackState(segment);
  const targetTrackId = getTranslationSubtitleTrackId(targetLanguage);
  const existingOriginalSegments = normalized.subtitleTracks
    ?.find((track) => track.id === ORIGINAL_SUBTITLE_TRACK_ID)
    ?.segments ?? [];
  const existingTargetSegments = normalized.subtitleTracks
    ?.find((track) => track.id === targetTrackId)
    ?.segments ?? [];
  const originalTrack: SubtitleTrack = {
    id: ORIGINAL_SUBTITLE_TRACK_ID,
    kind: 'original',
    slotLabel: null,
    targetLanguage: null,
    segments: mergeS2sSubtitleSegments(existingOriginalSegments, sourceSegments),
  };
  const withOriginal = upsertSubtitleTrack(normalized, originalTrack);
  if (targetSegments.length === 0) return withOriginal;
  const withTranslation = upsertSubtitleTrack(withOriginal, {
    id: targetTrackId,
    kind: 'translation',
    slotLabel: null,
    targetLanguage,
    segments: mergeS2sSubtitleSegments(existingTargetSegments, targetSegments),
  });
  return setActiveSubtitleTrackView(withTranslation, targetTrackId);
}
