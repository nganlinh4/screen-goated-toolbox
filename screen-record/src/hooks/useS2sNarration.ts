import { useCallback, useEffect, useRef, useState } from 'react';
import type { Translations } from '@/i18n';
import { invoke, logToHost } from '@/lib/ipc';
import { useAsyncJobPoll, buildCancelHandler } from './useAsyncJobPoll';
import {
  buildSubtitleGenerationPlan,
  type SubtitleGenerationPlan,
  type SubtitleSource,
} from '@/lib/subtitleGenerationPlan';
import {
  ORIGINAL_SUBTITLE_TRACK_ID,
  getTranslationSubtitleTrackId,
  normalizeSubtitleTrackState,
} from '@/lib/subtitleTracks';
import type {
  NarrationSegment,
  ProjectComposition,
  SubtitleSegment,
  VideoSegment,
} from '@/types/video';
import type { TrackSelectionRange } from '@/lib/timelineSegmentSelection';
import {
  buildNarration,
  buildSubtitle,
  cloneSubtitleSnapshot,
  localizeS2sStatus,
  replaceS2sSubtitleSegments,
  s2sSubtitleId,
  type PopulateS2sSubtitleTracksOptions,
  type S2sNarrationStatus,
  type S2sSubtitleStateSnapshot,
} from './s2sNarrationSubtitles';

export {
  populateEmptyS2sSubtitleTracks,
  type PopulateS2sSubtitleTracksOptions,
} from './s2sNarrationSubtitles';

interface UseS2sNarrationParams {
  backendMode?: 's2s' | 'gemini-translate';
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
    options?: PopulateS2sSubtitleTracksOptions,
  ) => VideoSegment | void;
  onFinalize: () => void | Promise<void>;
}

const BACKEND_COMMANDS = {
  s2s: {
    start: 'start_s2s_narration',
    status: 'get_s2s_narration_status',
    cancel: 'cancel_s2s_narration',
    batchPrefix: 's2s-narration',
    stallTag: 'S2SNarration',
    ttsMethod: 'gemini-live-s2s',
  },
  'gemini-translate': {
    start: 'start_gemini_translate_narration',
    status: 'get_gemini_translate_narration_status',
    cancel: 'cancel_gemini_translate_narration',
    batchPrefix: 'gemini-translate-narration',
    stallTag: 'GeminiTranslateNarration',
    ttsMethod: 'gemini-live-translate',
  },
} as const;

export function useS2sNarration({
  backendMode = 's2s',
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

  useEffect(() => {
    latestRefs.current = {
      t,
      onApplyNarrationSegments,
      onPopulateEmptySubtitles,
      onFinalize,
    };
  }, [onApplyNarrationSegments, onFinalize, onPopulateEmptySubtitles, t]);

  useAsyncJobPoll<S2sNarrationStatus>({
    jobId,
    restartKey: backendMode,
    fetchStatus: (activeJobId) =>
      invoke<S2sNarrationStatus>(BACKEND_COMMANDS[backendMode].status, {
        jobId: activeJobId,
        knownResultsRevision: revisionRef.current,
      }),
    isTerminal: (next) =>
      next.state === 'completed'
      || next.state === 'cancelled'
      || next.state === 'error',
    onTick: async (next) => {
      const commands = BACKEND_COMMANDS[backendMode];
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
          `[${commands.stallTag}][FrontendStall] job=${jobId} state=${next.state} completed=${next.completedClips}/${next.totalClips} active=${next.activeClipId ?? ''} vad=${next.vadSegmentDone ?? 0}/${next.vadSegmentTotal ?? 0} revision=${next.resultsRevision} message=${next.message}`,
        );
      }
      revisionRef.current = Math.max(revisionRef.current, next.resultsRevision);
      const activeTargetLanguage = jobTargetLanguageRef.current;
      const latest = latestRefs.current;
      setStatus({ ...next, message: localizeS2sStatus(latest.t, next, backendMode), results: [] });
      const plan = planRef.current;
      if (plan && next.results.length > 0) {
        const flat = next.results.flatMap((result) => result.segments);
        const sourceSubtitles = flat.map((result) =>
          buildSubtitle(result, plan, 'source', result.sourceText),
        );
        const targetSubtitles = flat.map((result) =>
          buildSubtitle(result, plan, 'target', result.targetText),
        );
        const narrations = flat.map((result) =>
          buildNarration(result, plan, batchIdRef.current, commands.ttsMethod),
        );
        const replaceIds = flat.map((result) => s2sSubtitleId(result, 'target'));
        sourceSubtitleResultsRef.current = replaceS2sSubtitleSegments([
          ...sourceSubtitleResultsRef.current,
          ...sourceSubtitles,
        ]);
        targetSubtitleResultsRef.current = replaceS2sSubtitleSegments([
          ...targetSubtitleResultsRef.current,
          ...targetSubtitles,
        ]);
        latest.onPopulateEmptySubtitles(
          sourceSubtitleResultsRef.current,
          targetSubtitleResultsRef.current,
          activeTargetLanguage,
          {
            preserveExistingOutside: true,
            baseSourceSegments: baseSourceSubtitleResultsRef.current,
            baseTargetSegments: baseTargetSubtitleResultsRef.current,
            restoreSnapshot: baseSubtitleSnapshotRef.current,
            debugPhase: 'live',
            liveUpdate: true,
          },
        );
        await latest.onApplyNarrationSegments(narrations, replaceIds);
      }
    },
    onComplete: async (next) => {
      const activeTargetLanguage = jobTargetLanguageRef.current;
      const latest = latestRefs.current;
      if (next.state === 'completed') {
        latest.onPopulateEmptySubtitles(
          sourceSubtitleResultsRef.current,
          targetSubtitleResultsRef.current,
          activeTargetLanguage,
          {
            preserveExistingOutside: true,
            baseSourceSegments: baseSourceSubtitleResultsRef.current,
            baseTargetSegments: baseTargetSubtitleResultsRef.current,
            restoreSnapshot: baseSubtitleSnapshotRef.current,
            debugPhase: 'complete',
          },
        );
        setJobId(null);
        await latest.onFinalize();
        return;
      }
      // cancelled or error terminal state
      latest.onPopulateEmptySubtitles(
        [],
        [],
        activeTargetLanguage,
        {
          restoreSnapshot: baseSubtitleSnapshotRef.current,
          debugPhase: next.state,
        },
      );
      setJobId(null);
      await latest.onFinalize();
    },
    onError: async (error) => {
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
      latestRefs.current.onPopulateEmptySubtitles(
        [],
        [],
        jobTargetLanguageRef.current,
        {
          restoreSnapshot: baseSubtitleSnapshotRef.current,
          debugPhase: 'poll-error',
        },
      );
      setJobId(null);
      await latestRefs.current.onFinalize();
    },
    intervalFor: (next) => (next.results.length > 0 ? 250 : 600),
  });

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
      const commands = BACKEND_COMMANDS[backendMode];
      batchIdRef.current = `${commands.batchPrefix}-${Date.now()}`;
      jobTargetLanguageRef.current = targetLanguage;
      sourceSubtitleResultsRef.current = [];
      targetSubtitleResultsRef.current = [];
      baseSubtitleSnapshotRef.current = cloneSubtitleSnapshot(segment);
      const normalizedSegment = segment ? normalizeSubtitleTrackState(segment) : null;
      const targetTrackId = getTranslationSubtitleTrackId(targetLanguage);
      baseSourceSubtitleResultsRef.current =
        normalizedSegment?.subtitleTracks?.find((track) => track.id === ORIGINAL_SUBTITLE_TRACK_ID)?.segments ?? [];
      baseTargetSubtitleResultsRef.current =
        normalizedSegment?.subtitleTracks?.find((track) => track.id === targetTrackId)?.segments ?? [];
      revisionRef.current = 0;
      lastProgressSignatureRef.current = '';
      lastProgressAtRef.current = performance.now();
      lastStallLogAtRef.current = 0;
      const response = await invoke<{ jobId: string }>(commands.start, {
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
    backendMode,
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
    t.narrationGeminiTranslateQueued,
    t.subtitleStatusNoSource,
    targetLanguage,
  ]);

  const handleCancel = useCallback(
    buildCancelHandler({
      jobId,
      cancelCommand: BACKEND_COMMANDS[backendMode].cancel,
      onCancelled: async () => {
        latestRefs.current.onPopulateEmptySubtitles(
          [],
          [],
          jobTargetLanguageRef.current,
          {
            restoreSnapshot: baseSubtitleSnapshotRef.current,
            debugPhase: 'manual-cancel',
          },
        );
        setJobId(null);
        setStatus((prev) => prev ? { ...prev, state: 'cancelled', message: 'Cancelled' } : prev);
        await latestRefs.current.onFinalize();
      },
    }),
    [backendMode, jobId],
  );

  return {
    canGenerate: !jobId && !isStarting,
    isGenerating: Boolean(jobId) || isStarting,
    status,
    handleGenerate,
    handleCancel,
  };
}
