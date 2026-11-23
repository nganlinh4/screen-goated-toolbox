import type { NarrationSegment, SubtitleSegment } from '@/types/video';
import type { SubtitleGenerationPlan } from '@/lib/subtitleGenerationPlan';
import type { S2sNarrationStatus, S2sSubtitleStateSnapshot } from './s2sNarrationSubtitles';
import type { SubtitleSourceOwner } from './subtitleSourceOwnership';
import { logToHost } from '@/lib/ipc';
import type { S2sDestination } from './s2sSourceOwnership';

export const S2S_COMMANDS = {
  s2s: {
    start: 'start_s2s_narration', status: 'get_s2s_narration_status', cancel: 'cancel_s2s_narration',
    batchPrefix: 's2s-narration', stallTag: 'S2SNarration', ttsMethod: 'gemini-live-s2s',
  },
  'gemini-translate': {
    start: 'start_gemini_translate_narration', status: 'get_gemini_translate_narration_status',
    cancel: 'cancel_gemini_translate_narration', batchPrefix: 'gemini-translate-narration',
    stallTag: 'GeminiTranslateNarration', ttsMethod: 'gemini-live-translate',
  },
} as const;

export interface ActiveS2sRun {
  jobId: string;
  backendMode: 's2s' | 'gemini-translate';
  cancelCommand: string;
  shared: S2sNarrationRunState;
  status: S2sNarrationStatus | null;
  plan: SubtitleGenerationPlan;
  sourceOwnersByClip: Record<string, SubtitleSourceOwner>;
  batchId: string;
  targetLanguage: string;
  sourceSubtitleResults: SubtitleSegment[];
  targetSubtitleResults: SubtitleSegment[];
  baseSourceSubtitleResults: SubtitleSegment[];
  baseTargetSubtitleResults: SubtitleSegment[];
  baseSubtitleSnapshot: S2sSubtitleStateSnapshot | null;
  resultsRevision: number;
  lastProgressSignature: string;
  lastProgressAt: number;
  lastStallLogAt: number;
}

export interface PendingS2sApply {
  narrations: NarrationSegment[];
  replaceIds: string[];
  running: Promise<void> | null;
}

/** Shared across panel mounts until this exact backend job is finalized. */
export interface S2sNarrationRunState {
  destination: S2sDestination | null;
  subtitleRevisions?: string[];
  pending: PendingS2sApply | null;
  failure: unknown | null;
  completion: Promise<void> | null;
  retired: boolean;
  terminalStatus: S2sNarrationStatus | null;
  listeners: Set<() => void>;
}

export function createS2sNarrationRunState(destination: S2sDestination | null = null): S2sNarrationRunState {
  return { destination, pending: null, failure: null, completion: null, retired: false, terminalStatus: null, listeners: new Set() };
}

export function notifyS2sNarrationRun(state: S2sNarrationRunState): void {
  state.listeners.forEach((listener) => listener());
}

export function trackS2sProgress(
  next: S2sNarrationStatus, jobId: string, tag: string,
  signatureRef: React.MutableRefObject<string>, progressRef: React.MutableRefObject<number>,
  stallRef: React.MutableRefObject<number>,
) {
  const signature = [next.state, next.completedClips, next.activeClipId ?? '',
    next.vadSegmentDone ?? 0, next.vadSegmentTotal ?? 0, next.resultsRevision].join(':');
  const now = performance.now();
  if (signature !== signatureRef.current) {
    signatureRef.current = signature;
    progressRef.current = now;
  } else if (next.state === 'running' && now - progressRef.current > 15_000 && now - stallRef.current > 15_000) {
    stallRef.current = now;
    logToHost(`[${tag}][FrontendStall] job=${jobId} state=${next.state} completed=${next.completedClips}/${next.totalClips} active=${next.activeClipId ?? ''} vad=${next.vadSegmentDone ?? 0}/${next.vadSegmentTotal ?? 0} revision=${next.resultsRevision} message=${next.message}`);
  }
}
