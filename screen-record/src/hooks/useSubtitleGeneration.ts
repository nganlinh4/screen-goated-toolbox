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
import type { ProjectComposition, VideoSegment } from '@/types/video';

interface SubtitleClipPayload {
  clipId: string;
  clipName: string;
  sourcePath: string;
  sourceDuration: number;
  trimSegments: Array<{ id: string; startTime: number; endTime: number }>;
  micAudioOffsetSec?: number;
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

interface SubtitleJobStatus {
  state: 'queued' | 'running' | 'completed' | 'cancelled' | 'error';
  message: string;
  progress: number;
  activeClipId?: string | null;
  totalClips: number;
  completedClips: number;
  results: SubtitleClipResult[];
  skipped: SubtitleSkippedClip[];
  error?: string | null;
}

interface UseSubtitleGenerationParams {
  segment: VideoSegment | null;
  setSegment: (segment: VideoSegment | null | ((prev: VideoSegment | null) => VideoSegment | null)) => void;
  composition: ProjectComposition | null;
  setComposition: (composition: ProjectComposition | null | ((prev: ProjectComposition | null) => ProjectComposition | null)) => void;
  activeClipId: string | null | undefined;
  currentRawVideoPath: string;
  currentRawMicAudioPath: string;
  duration: number;
  setActivePanel: (panel: 'zoom' | 'background' | 'cursor' | 'text' | 'subtitles') => void;
}

function defaultSubtitleStyle() {
  return {
    fontSize: 54,
    color: '#ffffff',
    x: 50,
    y: 90,
    fontVariations: { wght: 600, wdth: 100, slnt: 0, ROND: 0 },
    textAlign: 'center' as const,
    opacity: 1,
    letterSpacing: 0,
    background: {
      enabled: true,
      color: '#000000',
      opacity: 0.65,
      paddingX: 16,
      paddingY: 8,
      borderRadius: 32,
    },
  };
}

function buildSubtitleId(clipId: string, entry: { startTime: number; endTime: number; text: string }, index: number) {
  return `subtitle-${clipId}-${Math.round(entry.startTime * 1000)}-${Math.round(entry.endTime * 1000)}-${index}`;
}

function buildClipPayloads({
  segment,
  composition,
  activeClipId,
  currentRawVideoPath,
  currentRawMicAudioPath,
  duration,
  sourceType,
}: {
  segment: VideoSegment | null;
  composition: ProjectComposition | null;
  activeClipId: string | null | undefined;
  currentRawVideoPath: string;
  currentRawMicAudioPath: string;
  duration: number;
  sourceType: 'video' | 'mic';
}): SubtitleClipPayload[] {
  const effectiveMode = getEffectiveCompositionMode(composition);
  if (!composition || effectiveMode === 'separate') {
    if (!segment) return [];
    const sourcePath = sourceType === 'mic' ? currentRawMicAudioPath : currentRawVideoPath;
    if (!sourcePath) return [];
    return [{
      clipId: activeClipId ?? 'root',
      clipName: 'Current Clip',
      sourcePath,
      sourceDuration: duration,
      trimSegments: segment.trimSegments ?? [{
        id: 'full',
        startTime: segment.trimStart,
        endTime: segment.trimEnd,
      }],
      micAudioOffsetSec: segment.micAudioOffsetSec,
    }];
  }

  return composition.clips.flatMap((clip) => {
    const sourcePath = sourceType === 'mic' ? (clip.rawMicAudioPath ?? '') : (clip.rawVideoPath ?? '');
    if (!sourcePath) return [];
    return [{
      clipId: clip.id,
      clipName: clip.name,
      sourcePath,
      sourceDuration: clip.duration,
      trimSegments: clip.segment.trimSegments ?? [{
        id: 'full',
        startTime: clip.segment.trimStart,
        endTime: clip.segment.trimEnd,
      }],
      micAudioOffsetSec: clip.segment.micAudioOffsetSec,
    }];
  });
}

export function useSubtitleGeneration({
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
  const [sourceType, setSourceType] = useState<'video' | 'mic'>('video');
  const [languageHint, setLanguageHint] = useState('auto');
  const [jobId, setJobId] = useState<string | null>(null);
  const [status, setStatus] = useState<SubtitleJobStatus | null>(null);

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

  const applyResults = useCallback((results: SubtitleClipResult[]) => {
    const subtitleStyle = defaultSubtitleStyle();
    if (!composition || getEffectiveCompositionMode(composition) === 'separate') {
      const rootResult = results[0];
      if (!segment || !rootResult) return;
      setSegment({
        ...segment,
        subtitleSegments: rootResult.segments.map((entry, index) => ({
          id: buildSubtitleId(rootResult.clipId, entry, index),
          startTime: entry.startTime,
          endTime: entry.endTime,
          text: entry.text,
          style: subtitleStyle,
        })),
      });
      return;
    }

    const timeline = buildSequenceTimeline(composition);
    if (!timeline) return;
    const resultsByClip = new Map(results.map((result) => [result.clipId, result]));

    setComposition((prev) => {
      if (!prev) return prev;
      let next = prev;
      for (const clip of prev.clips) {
        const result = resultsByClip.get(clip.id);
        if (!result) continue;
        const updatedSegment = {
          ...clip.segment,
          subtitleSegments: result.segments.map((entry, index) => ({
            id: buildSubtitleId(clip.id, entry, index),
            startTime: entry.startTime,
            endTime: entry.endTime,
            text: entry.text,
            style: subtitleStyle,
          })),
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
  }, [composition, segment, setComposition, setSegment]);

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
        const nextStatus = await invoke<SubtitleJobStatus>('get_subtitle_generation_status', { jobId });
        if (cancelled) return;
        setStatus(nextStatus);
        if (nextStatus.results.length > 0) {
          applyResults(nextStatus.results);
        }
        if (nextStatus.state === 'completed') {
          applyResults(nextStatus.results);
          setJobId(null);
          setActivePanel('subtitles');
        } else if (nextStatus.state === 'cancelled' || nextStatus.state === 'error') {
          setJobId(null);
        } else {
          window.setTimeout(poll, 400);
        }
      } catch (error) {
        if (!cancelled) {
          setStatus({
            state: 'error',
            message: error instanceof Error ? error.message : 'Subtitle generation failed',
            progress: 0,
            activeClipId: null,
            totalClips: 0,
            completedClips: 0,
            results: [],
            skipped: [],
            error: error instanceof Error ? error.message : String(error),
          });
          setJobId(null);
        }
      }
    };
    void poll();
    return () => {
      cancelled = true;
    };
  }, [jobId, applyResults, setActivePanel]);

  const handleGenerateSubtitles = useCallback(async () => {
    const clips = buildClipPayloads({
      segment,
      composition,
      activeClipId,
      currentRawVideoPath,
      currentRawMicAudioPath,
      duration,
      sourceType,
    });
    if (clips.length === 0) {
      setStatus({
        state: 'error',
        message: 'No subtitle source available',
        progress: 0,
        activeClipId: null,
        totalClips: 0,
        completedClips: 0,
        results: [],
        skipped: [],
        error: 'No subtitle source available',
      });
      return;
    }
    setActivePanel('subtitles');
    const result = await invoke<{ jobId: string }>('start_subtitle_generation', {
      sourceType,
      languageHint: languageHint.trim() || 'auto',
      clips,
    });
    setStatus({
      state: 'queued',
      message: 'Starting subtitle generation…',
      progress: 0,
      activeClipId: null,
      totalClips: clips.length,
      completedClips: 0,
      results: [],
      skipped: [],
      error: null,
    });
    setJobId(result.jobId);
  }, [segment, composition, activeClipId, currentRawVideoPath, currentRawMicAudioPath, duration, sourceType, languageHint, setActivePanel]);

  const handleCancelSubtitleGeneration = useCallback(async () => {
    if (!jobId) return;
    await invoke('cancel_subtitle_generation', { jobId });
    setStatus((prev) => prev ? { ...prev, state: 'cancelled', message: 'Cancelled' } : prev);
    setJobId(null);
  }, [jobId]);

  return {
    editingSubtitleId,
    setEditingSubtitleId,
    subtitleSource: sourceType,
    setSubtitleSource: setSourceType,
    subtitleLanguageHint: languageHint,
    setSubtitleLanguageHint: setLanguageHint,
    isGeneratingSubtitles: !!jobId,
    subtitleStatusMessage: status?.message ?? null,
    subtitleActiveClipId: status?.activeClipId ?? null,
    canUseVideoSubtitleSource: canUseVideoSource,
    canUseMicSubtitleSource: canUseMicSource,
    handleGenerateSubtitles,
    handleCancelSubtitleGeneration,
  };
}
