import type { VideoSegment } from '@/types/video';
import { cloneSubtitleSnapshot } from './s2sNarrationSubtitles';
import type { S2sNarrationRunState } from './s2sNarrationRunState';
import type { SubtitleSourceSnapshot } from './subtitleSourceOwnership';
import type { SubtitleGenerationPlan } from '@/lib/subtitleGenerationPlan';
import { buildSequenceTimeline, mergeCompositionSegmentsToSequence } from '@/lib/sequenceTimeline';

export function s2sSubtitleCoordinateSpace(plan: SubtitleGenerationPlan): 'clip' | 'sequence' {
  return Object.keys(plan.clipTransformsByClip).length > 0 ? 'sequence' : 'clip';
}

export function s2sSubtitleDestination(source: SubtitleSourceSnapshot, plan: SubtitleGenerationPlan): VideoSegment | null {
  if (s2sSubtitleCoordinateSpace(plan) === 'clip') return source.localSegment ?? source.segment;
  if (!source.composition) return null;
  const timeline = buildSequenceTimeline(source.composition);
  return source.composition.globalSegment ?? (timeline ? mergeCompositionSegmentsToSequence(timeline) : null);
}

export function s2sSubtitleRevision(segment: VideoSegment | null): string {
  return JSON.stringify(cloneSubtitleSnapshot(segment));
}

/** Permit only the last observed revision or a subsequently issued job write. */
export function s2sSubtitlesCurrent(state: S2sNarrationRunState, segment: VideoSegment | null): boolean {
  const revisions = state.subtitleRevisions;
  if (!revisions) return false;
  const index = revisions.lastIndexOf(s2sSubtitleRevision(segment));
  if (index < 0) return false;
  if (index > 0) revisions.splice(0, index);
  return true;
}

export function recordS2sSubtitleWrite(state: S2sNarrationRunState, segment: VideoSegment | void): void {
  if (!segment || !state.subtitleRevisions) return;
  const revision = s2sSubtitleRevision(segment);
  if (state.subtitleRevisions[state.subtitleRevisions.length - 1] !== revision) state.subtitleRevisions.push(revision);
}
