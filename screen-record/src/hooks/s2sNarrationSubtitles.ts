import type { Translations } from "@/i18n";
import { defaultSubtitleStyle } from "@/lib/subtitleDefaults";
import { formatTemplate } from "@/lib/statusFormat";
import type { SubtitleGenerationPlan } from "@/lib/subtitleGenerationPlan";
import {
  ORIGINAL_SUBTITLE_TRACK_ID,
  getTranslationSubtitleTrackId,
  normalizeSubtitleTrackState,
  setActiveSubtitleTrackView,
  upsertSubtitleTrack,
} from "@/lib/subtitleTracks";
import type { BaseAsyncJobStatus } from "@/hooks/asyncJobTypes";
import type {
  NarrationSegment,
  SubtitleChainItem,
  SubtitleSegment,
  SubtitleTrack,
  SubtitleViewState,
  VideoSegment,
} from "@/types/video";

export interface S2sNarrationSegmentResult {
  id: string;
  clipId: string;
  sourceText: string;
  targetText: string;
  startTime: number;
  endTime: number;
  narrationStartTime?: number;
  path: string;
  duration: number;
  audioInPoint?: number;
  audioOutPoint?: number;
  narrationGroupTakeId?: string;
  narrationGroupSourceStartTime?: number;
  alignmentMode?: NarrationSegment["narrationAlignmentMode"];
  alignmentConfidence?: number;
  ttsProfileMethod?: string;
}

export interface S2sNarrationClipResult {
  clipId: string;
  isPartial: boolean;
  segments: S2sNarrationSegmentResult[];
}

export interface S2sNarrationStatus extends BaseAsyncJobStatus {
  totalClips: number;
  completedClips: number;
  activeClipId?: string | null;
  vadSegmentDone?: number;
  vadSegmentTotal?: number;
  vadNoSpeech?: boolean;
  resultsRevision: number;
  results: S2sNarrationClipResult[];
}

export interface S2sSubtitleStateSnapshot {
  subtitleTracks?: SubtitleTrack[];
  activeSubtitleView?: SubtitleViewState;
  subtitleCustomChain?: SubtitleChainItem[];
  subtitleSegments?: SubtitleSegment[];
}

export interface PopulateS2sSubtitleTracksOptions {
  preserveExistingOutside?: boolean;
  baseSourceSegments?: SubtitleSegment[];
  baseTargetSegments?: SubtitleSegment[];
  replacementRanges?: Array<{ startTime: number; endTime: number }>;
  restoreSnapshot?: S2sSubtitleStateSnapshot | null;
  debugPhase?: string;
  liveUpdate?: boolean;
}

export function mergeS2sSubtitleSegments(
  existingSegments: readonly SubtitleSegment[],
  incomingSegments: readonly SubtitleSegment[],
  replacementRanges?: readonly { startTime: number; endTime: number }[],
) {
  const incomingIds = new Set(incomingSegments.map((segment) => segment.id));
  const normalizedRanges = replacementRanges
    ?.map((range) => ({
      startTime: Math.min(range.startTime, range.endTime),
      endTime: Math.max(range.startTime, range.endTime),
    }))
    .filter((range) => range.endTime - range.startTime > 0.001);
  if (incomingSegments.length === 0 && (!normalizedRanges || normalizedRanges.length === 0)) {
    return [...existingSegments];
  }
  const fallbackRanges =
    incomingSegments.length > 0
      ? incomingSegments.map((segment) => ({
          startTime: segment.startTime,
          endTime: segment.endTime,
        }))
      : [];
  const ranges =
    normalizedRanges && normalizedRanges.length > 0
      ? normalizedRanges
      : fallbackRanges;
  const epsilon = 0.001;
  const kept = existingSegments.filter((segment) => {
    if (incomingIds.has(segment.id)) return false;
    return !ranges.some(
      (range) =>
        segment.startTime < range.endTime - epsilon &&
        range.startTime + epsilon < segment.endTime,
    );
  });
  return [...kept, ...incomingSegments].sort((left, right) =>
    left.startTime === right.startTime
      ? left.endTime - right.endTime
      : left.startTime - right.startTime,
  );
}

export function replaceS2sSubtitleSegments(
  incomingSegments: readonly SubtitleSegment[],
) {
  const byId = new Map<string, SubtitleSegment>();
  for (const segment of incomingSegments) {
    byId.set(segment.id, segment);
  }
  return [...byId.values()].sort((left, right) =>
    left.startTime === right.startTime
      ? left.endTime - right.endTime
      : left.startTime - right.startTime,
  );
}

function cloneSubtitleSegment(segment: SubtitleSegment): SubtitleSegment {
  return {
    ...segment,
    style: segment.style ? { ...segment.style } : segment.style,
    sourceGroup: segment.sourceGroup ? { ...segment.sourceGroup } : segment.sourceGroup,
    provenance: segment.provenance ? { ...segment.provenance } : segment.provenance,
  };
}

export function cloneSubtitleSnapshot(
  segment: VideoSegment | null,
): S2sSubtitleStateSnapshot | null {
  if (!segment) return null;
  const normalized = normalizeSubtitleTrackState(segment);
  return {
    subtitleTracks: normalized.subtitleTracks?.map((track) => ({
      ...track,
      segments: track.segments.map(cloneSubtitleSegment),
    })),
    activeSubtitleView: normalized.activeSubtitleView
      ? { ...normalized.activeSubtitleView }
      : undefined,
    subtitleCustomChain: normalized.subtitleCustomChain?.map((item) => ({ ...item })),
    subtitleSegments: normalized.subtitleSegments?.map(cloneSubtitleSegment),
  };
}

function restoreSubtitleSnapshot(
  segment: VideoSegment,
  snapshot: S2sSubtitleStateSnapshot,
): VideoSegment {
  return normalizeSubtitleTrackState({
    ...segment,
    subtitleTracks: snapshot.subtitleTracks?.map((track) => ({
      ...track,
      segments: track.segments.map(cloneSubtitleSegment),
    })),
    activeSubtitleView: snapshot.activeSubtitleView
      ? { ...snapshot.activeSubtitleView }
      : undefined,
    subtitleCustomChain: snapshot.subtitleCustomChain?.map((item) => ({ ...item })),
    subtitleSegments: snapshot.subtitleSegments?.map(cloneSubtitleSegment),
  });
}

export function populateEmptyS2sSubtitleTracks(
  segment: VideoSegment,
  sourceSegments: SubtitleSegment[],
  targetSegments: SubtitleSegment[],
  targetLanguage: string,
  options: PopulateS2sSubtitleTracksOptions = {},
): VideoSegment {
  const sourceSegment = options.restoreSnapshot
    ? restoreSubtitleSnapshot(segment, options.restoreSnapshot)
    : segment;
  if (sourceSegments.length === 0 && targetSegments.length === 0) {
    if (options.restoreSnapshot) return sourceSegment;
    return normalizeSubtitleTrackState(sourceSegment);
  }
  const normalized = normalizeSubtitleTrackState(sourceSegment);
  const targetTrackId = getTranslationSubtitleTrackId(targetLanguage);
  const existingOriginalSegments =
    normalized.subtitleTracks?.find((track) => track.id === ORIGINAL_SUBTITLE_TRACK_ID)
      ?.segments ?? [];
  const existingTargetSegments =
    normalized.subtitleTracks?.find((track) => track.id === targetTrackId)
      ?.segments ?? [];
  const sourceBaseSegments = options.baseSourceSegments ?? existingOriginalSegments;
  const targetBaseSegments = options.baseTargetSegments ?? existingTargetSegments;
  const nextSourceSegments = options.preserveExistingOutside
    ? mergeS2sSubtitleSegments(sourceBaseSegments, sourceSegments, options.replacementRanges)
    : replaceS2sSubtitleSegments(sourceSegments);
  const nextTargetSegments = options.preserveExistingOutside
    ? mergeS2sSubtitleSegments(targetBaseSegments, targetSegments, options.replacementRanges)
    : replaceS2sSubtitleSegments(targetSegments);
  const originalTrack: SubtitleTrack = {
    id: ORIGINAL_SUBTITLE_TRACK_ID,
    kind: "original",
    slotLabel: null,
    targetLanguage: null,
    segments: nextSourceSegments,
  };
  const withOriginal = upsertSubtitleTrack(normalized, originalTrack);
  if (targetSegments.length === 0) return withOriginal;
  const withTranslation = upsertSubtitleTrack(withOriginal, {
    id: targetTrackId,
    kind: "translation",
    slotLabel: null,
    targetLanguage,
    segments: nextTargetSegments,
  });
  return setActiveSubtitleTrackView(withTranslation, targetTrackId);
}

export function mappedTime(
  result: S2sNarrationSegmentResult,
  plan: SubtitleGenerationPlan,
  field: "startTime" | "endTime",
) {
  const transform = plan.clipTransformsByClip[result.clipId];
  if (!transform) return result[field];
  return result[field] + transform.timelineOffsetSec;
}

export function s2sSubtitleId(
  result: S2sNarrationSegmentResult,
  kind: "source" | "target",
) {
  return `${result.id}-${kind}`;
}

export function buildSubtitle(
  result: S2sNarrationSegmentResult,
  plan: SubtitleGenerationPlan,
  kind: "source" | "target",
  text: string,
): SubtitleSegment {
  const transform = plan.clipTransformsByClip[result.clipId];
  const startTime = mappedTime(result, plan, "startTime");
  const endTime = Math.max(startTime + 0.05, mappedTime(result, plan, "endTime"));
  const base: SubtitleSegment = {
    id: s2sSubtitleId(result, kind),
    startTime,
    endTime,
    text,
    style: defaultSubtitleStyle(),
    sourceGroup: {
      kind: transform ? "audio" : plan.sourceTypeForNative,
      assignment: "generated",
      audioSegmentId: transform?.audioSegmentId,
      sourceName: transform?.sourceName,
      sourcePath: transform?.sourcePath,
    },
  };
  if (!transform) return base;
  return {
    ...base,
    provenance: {
      sourceKind: "audio",
      audioSegmentId: transform.audioSegmentId,
      sourceName: transform.sourceName,
      sourcePath: transform.sourcePath,
      sourceLocalStartTime: Math.max(0, result.startTime - transform.sourceLocalOffsetSec),
      sourceLocalEndTime: Math.max(0, result.endTime - transform.sourceLocalOffsetSec),
    },
  };
}

export function buildNarration(
  result: S2sNarrationSegmentResult,
  plan: SubtitleGenerationPlan,
  batchId: string,
  defaultTtsMethod: string,
): NarrationSegment {
  const startTime = Number.isFinite(result.narrationStartTime)
    ? mappedTime({ ...result, startTime: result.narrationStartTime! }, plan, "startTime")
    : mappedTime(result, plan, "startTime");
  const name = result.targetText.trim() || result.sourceText.trim() || "Gemini S2S";
  const targetSubtitleId = s2sSubtitleId(result, "target");
  const inPoint = Math.max(0, result.audioInPoint ?? 0);
  const outPoint = Math.max(inPoint + 0.05, result.audioOutPoint ?? result.duration);
  return {
    id: `${batchId}-${result.id}`,
    rawAudioPath: result.path,
    name: name.slice(0, 42),
    duration: Math.max(0.05, result.duration),
    startTime,
    inPoint,
    outPoint,
    playbackRate: 1,
    addedAt: Date.now(),
    sourceSubtitleId: targetSubtitleId,
    sourceSubtitleIds: [targetSubtitleId],
    narrationBatchId: batchId,
    narrationGroupTakeId: result.narrationGroupTakeId,
    narrationGroupSourceStartTime: result.narrationGroupSourceStartTime,
    narrationAlignmentMode: result.alignmentMode ?? "single",
    narrationAlignmentConfidence: result.alignmentConfidence ?? 1,
    ttsProfileSnapshot: {
      method: result.ttsProfileMethod ?? defaultTtsMethod,
    },
  };
}

export function localizeS2sStatus(
  t: Translations,
  status: S2sNarrationStatus,
  backendMode: "s2s" | "gemini-translate",
) {
  const totalClips = Math.max(1, status.totalClips || 1);
  const activeClip = Math.min(totalClips, Math.max(1, status.completedClips + 1));
  const strings = backendMode === "gemini-translate"
    ? {
      queued: t.narrationGeminiTranslateQueued,
      running: t.narrationGeminiTranslateRunning,
      vad: t.narrationGeminiTranslateVadProgress,
      noSpeech: t.narrationGeminiTranslateNoSpeech,
      complete: t.narrationGeminiTranslateComplete,
      failed: t.narrationGeminiTranslateFailed,
      starting: t.narrationGeminiTranslateStarting,
    }
    : {
      queued: t.narrationS2sQueued,
      running: t.narrationS2sRunning,
      vad: t.narrationS2sVadProgress,
      noSpeech: t.narrationS2sNoSpeech,
      complete: t.narrationS2sComplete,
      failed: t.narrationS2sFailed,
      starting: t.narrationS2sStarting,
    };
  if (status.state === "queued") return strings.queued;
  if (status.state === "completed") return strings.complete;
  if (status.state === "cancelled") return t.subtitleNarrationStatusCancelled;
  if (status.state === "error") return status.error || status.message || strings.failed;
  if (status.vadNoSpeech) {
    return formatTemplate(strings.noSpeech, {
      clip: activeClip,
      clips: totalClips,
    });
  }
  if (backendMode === "gemini-translate" && (status.vadSegmentDone ?? 0) > 0) {
    return formatTemplate(t.narrationGeminiTranslateLiveVadProgress, {
      clip: activeClip,
      clips: totalClips,
      done: status.vadSegmentDone ?? 0,
    });
  }
  if ((status.vadSegmentTotal ?? 0) > 0) {
    return formatTemplate(strings.vad, {
      clip: activeClip,
      clips: totalClips,
      done: status.vadSegmentDone ?? 0,
      total: status.vadSegmentTotal ?? 0,
    });
  }
  return status.state === "running"
    ? formatTemplate(strings.running, { clip: activeClip, clips: totalClips })
    : strings.starting;
}
