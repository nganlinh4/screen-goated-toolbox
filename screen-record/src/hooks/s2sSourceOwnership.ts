import { subtitleSourceMatches, type SubtitleSourceOwner, type SubtitleSourceSnapshot } from './subtitleSourceOwnership';
import type { SubtitleGenerationPlan } from '@/lib/subtitleGenerationPlan';
import type { S2sNarrationStatus } from './s2sNarrationSubtitles';
import type { ActiveS2sRun, S2sNarrationRunState } from './s2sNarrationRunState';
import { notifyS2sNarrationRun } from './s2sNarrationRunState';
import { retireNarrationJob } from './retiredNarrationCancellation';
import { retireNarrationBatch } from '@/lib/narrationBatchRetirements';
import { clearResumableRunIfJob } from './resumableJobRegistry';
import { getEffectiveCompositionMode } from '@/lib/projectComposition';

export interface S2sDestination {
  clipId: string | null;
  videoPath: string;
  storedVideoPath: string | null;
  clipDuration: number | null;
  clipOrder: string[];
  trim: string;
}

/** S2S currently writes to one live editor, not to per-clip destinations. */
export function captureS2sDestination(source: SubtitleSourceSnapshot): S2sDestination | null {
  if (!source.segment || getEffectiveCompositionMode(source.composition) === 'unified') return null;
  const clips = source.composition?.clips;
  const clipId = source.loadedClipId ?? (clips?.length === 1 ? clips[0].id : null);
  const clip = clipId ? clips?.find((item) => item.id === clipId) : undefined;
  if (clips && (!clipId || !clip)) return null;
  if (clip?.rawVideoPath && clip.rawVideoPath !== source.videoPath) return null;
  return {
    clipId, videoPath: source.videoPath, storedVideoPath: clip?.rawVideoPath ?? null,
    clipDuration: clip?.duration ?? null, clipOrder: clips?.map((item) => item.id) ?? [],
    trim: JSON.stringify([source.segment.trimStart, source.segment.trimEnd,
      source.segment.trimSegments?.map(({ startTime, endTime }) => [startTime, endTime]) ?? null]),
  };
}

export function s2sDestinationMatches(source: SubtitleSourceSnapshot, destination: S2sDestination | null | undefined): boolean {
  const current = captureS2sDestination(source);
  return !!destination && !!current && JSON.stringify(destination) === JSON.stringify(current);
}

export function s2sSourcesCurrent(
  getSnapshot: () => SubtitleSourceSnapshot, plan: SubtitleGenerationPlan,
  owners: Record<string, SubtitleSourceOwner>, destination: S2sDestination | null | undefined,
): boolean {
  const source = getSnapshot();
  return s2sDestinationMatches(source, destination)
    && plan.clips.length > 0 && plan.clips.every((clip) => subtitleSourceMatches(
    owners[clip.clipId], source.composition, source.loadedClipId, source.segment, source.videoPath, source.micPath,
  ));
}

export function s2sSourceChangedStatus(message: string): S2sNarrationStatus {
  return { state: 'error', message, error: message, progress: 0,
    totalClips: 0, completedClips: 0, resultsRevision: 0, results: [] };
}

export function retireStaleS2sRun(
  id: string, expected: S2sNarrationRunState, saved: ActiveS2sRun,
  namespace: string, message: string, revision: number,
): S2sNarrationStatus {
  expected.pending = null;
  expected.retired = true;
  retireNarrationJob(saved.cancelCommand, id);
  retireNarrationBatch(saved.batchId);
  clearResumableRunIfJob(namespace, id);
  const status: S2sNarrationStatus = { state: 'error', message, error: message, progress: 0,
    totalClips: 0, completedClips: 0, resultsRevision: revision, results: [] };
  expected.terminalStatus = status;
  notifyS2sNarrationRun(expected);
  return status;
}
