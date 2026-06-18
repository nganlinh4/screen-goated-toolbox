import type { NarrationSegment, TtsProfileSnapshot } from "@/types/video";
import type { NarrationProfilePayload } from "@/hooks/useNarrationSettings";
import type { BaseAsyncJobStatus } from "@/hooks/asyncJobTypes";

export interface SubtitleNarrationRequestItem {
  id: string;
  text: string;
  startTime: number;
  endTime: number;
  sourceSubtitleId: string;
  replaceSubtitleIds: string[];
}

export interface SubtitleNarrationResultItem {
  subtitleId: string;
  text: string;
  path: string;
  duration: number;
  sourceInPoint?: number;
  sourceOutPoint?: number;
  groupId?: string;
  narrationGroupTakeId?: string;
  narrationGroupPromptText?: string;
  narrationGroupSourceStartTime?: number;
  alignmentMode?: NarrationSegment["narrationAlignmentMode"];
  alignmentConfidence?: number;
  startTime: number;
  endTime: number;
  sourceSubtitleId?: string;
  replaceSubtitleIds?: string[];
}

export interface SubtitleNarrationJobStatus extends BaseAsyncJobStatus {
  totalItems: number;
  completedItems: number;
  activeSubtitleId?: string | null;
  resultsRevision?: number;
  results: SubtitleNarrationResultItem[];
  errors: Array<{ subtitleId: string; message: string }>;
}

export interface SubtitleNarrationGroupPreview {
  groups: Record<string, number>;
  groupCount: number;
}

export const DEFAULT_NARRATION_GROUP_TEXT_BUDGET = 25;
export const MIN_NARRATION_GROUP_TEXT_BUDGET = 5;
export const MAX_NARRATION_GROUP_TEXT_BUDGET = 120;
export const NARRATION_GROUP_VAD_RADIUS_SEC = 0.35;

// Target length (seconds) the Gemini Translate cues/takes are steered toward:
// long pause-free reads are split, short ones merged, so segments land near this
// without any too short. MIN/MAX bound the user-facing slider.
export const DEFAULT_NARRATION_TARGET_SEGMENT_SEC = 4;
export const MIN_NARRATION_TARGET_SEGMENT_SEC = 2;
export const MAX_NARRATION_TARGET_SEGMENT_SEC = 8;

const NARRATION_GROUP_MAX_ITEMS = 10;
const NARRATION_GROUP_MAX_CHARS = 650;
const NARRATION_GROUP_GAP_BREAK_SEC = 1.2;

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

export function buildNarrationSegment(
  result: SubtitleNarrationResultItem,
  batchId: string,
  profile: NarrationProfilePayload,
): NarrationSegment {
  const duration = Math.max(0.05, result.duration);
  const inPoint = Number.isFinite(result.sourceInPoint)
    ? Math.max(0, Math.min(duration, result.sourceInPoint ?? 0))
    : 0;
  const outPoint = Number.isFinite(result.sourceOutPoint)
    ? Math.max(inPoint + 0.05, Math.min(duration, result.sourceOutPoint ?? duration))
    : duration;
  return {
    id: `${batchId}-${result.groupId ?? "item"}-${result.subtitleId}`,
    rawAudioPath: result.path,
    name: result.text.trim().slice(0, 42) || "Narration",
    duration,
    startTime: Math.max(0, result.startTime),
    inPoint,
    outPoint,
    playbackRate: 1,
    addedAt: Date.now(),
    sourceSubtitleId: result.sourceSubtitleId ?? result.subtitleId,
    sourceSubtitleIds: result.replaceSubtitleIds,
    narrationBatchId: batchId,
    narrationGroupTakeId: result.narrationGroupTakeId,
    narrationGroupPromptText: result.narrationGroupPromptText,
    narrationGroupSourceStartTime: Number.isFinite(result.narrationGroupSourceStartTime)
      ? result.narrationGroupSourceStartTime
      : result.startTime - inPoint,
    narrationAlignmentMode: result.alignmentMode,
    narrationAlignmentConfidence: result.alignmentConfidence,
    ttsProfileSnapshot: profileToSnapshot(profile),
  };
}

export function countNarrationOverlaps(
  results: readonly SubtitleNarrationResultItem[],
) {
  const sorted = [...results].sort((a, b) => a.startTime - b.startTime);
  let count = 0;
  for (let index = 0; index < sorted.length - 1; index += 1) {
    const current = sorted[index];
    const next = sorted[index + 1];
    const currentVisibleDuration = Math.max(
      0.05,
      (current.sourceOutPoint ?? current.duration) - (current.sourceInPoint ?? 0),
    );
    if (current.startTime + currentVisibleDuration > next.startTime + 0.05) {
      count += 1;
    }
  }
  return count;
}

function estimateSpeechUnits(text: string) {
  const cleaned = text
    .normalize("NFC")
    .replace(/[♪♫♩♬♭♮♯]+/g, " ")
    .replace(/[^\p{L}\p{N}\s]+/gu, " ")
    .trim();
  if (!cleaned) return 0;
  const words = cleaned.match(/[\p{L}\p{N}]+/gu) ?? [];
  const hasUsefulSpaces = /\s/.test(cleaned) && words.length > 1;
  if (hasUsefulSpaces) return Math.max(1, words.length);
  const alnumChars = [...cleaned].filter((ch) => /[\p{L}\p{N}]/u.test(ch)).length;
  return Math.max(1, Math.ceil(alnumChars / 4));
}

export function buildNarrationGroupPreview(
  items: readonly SubtitleNarrationRequestItem[],
  textBudget: number,
): SubtitleNarrationGroupPreview {
  const safeBudget = Math.max(
    MIN_NARRATION_GROUP_TEXT_BUDGET,
    Math.min(MAX_NARRATION_GROUP_TEXT_BUDGET, Math.round(textBudget)),
  );
  const groups: Record<string, number> = {};
  let groupIndex = -1;
  let groupUnits = 0;
  let groupChars = 0;
  let groupItems = 0;
  let previousEnd: number | null = null;

  for (const item of items) {
    const units = estimateSpeechUnits(item.text);
    const chars = item.text.trim().length;
    if (units <= 0 || chars <= 0) continue;
    const gap = previousEnd === null ? 0 : item.startTime - previousEnd;
    const shouldStartNew =
      groupIndex < 0 ||
      groupItems >= NARRATION_GROUP_MAX_ITEMS ||
      groupChars + chars > NARRATION_GROUP_MAX_CHARS ||
      gap > NARRATION_GROUP_GAP_BREAK_SEC ||
      (groupItems > 0 && groupUnits + units > safeBudget);
    if (shouldStartNew) {
      groupIndex += 1;
      groupUnits = 0;
      groupChars = 0;
      groupItems = 0;
    }
    groups[item.id] = groupIndex;
    groupUnits += units;
    groupChars += chars;
    groupItems += 1;
    previousEnd = item.endTime;
  }

  return {
    groups,
    groupCount: groupIndex + 1,
  };
}
