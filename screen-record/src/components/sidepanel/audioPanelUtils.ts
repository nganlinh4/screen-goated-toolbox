import type { ImportedAudioSegment, NarrationSegment } from '@/types/video';
import type { ImportedAudioDraft } from '@/lib/importedAudioDraftStore';

export type AudioPanelSegment =
  | (ImportedAudioSegment & { kind: 'imported' })
  | (NarrationSegment & { kind: 'narration' });

export type AudioPanelDraft = ImportedAudioDraft;

export const RATE_MIN = 0.25;
export const RATE_MAX = 4;

export function clampRate(rate: number) {
  if (!Number.isFinite(rate)) return 1;
  return Math.min(RATE_MAX, Math.max(RATE_MIN, rate));
}

export function formatSec(value: number) {
  return value.toFixed(2);
}

export function getTimelineDuration(segment: AudioPanelSegment) {
  return Math.max(0.05, (segment.outPoint - segment.inPoint) / clampRate(segment.playbackRate ?? 1));
}
