import type { ImportedAudioSegment } from '@/types/video';

/** The playback rate used to map source-audio seconds to timeline seconds. */
export function audioSegmentPlaybackRate(rate: number | undefined): number {
  return typeof rate === 'number' && Number.isFinite(rate) && rate > 0 ? rate : 1;
}
export function appendAudioSegments(
  existing: readonly ImportedAudioSegment[],
  incoming: readonly ImportedAudioSegment[],
): ImportedAudioSegment[] {
  const duration = (segment: ImportedAudioSegment) =>
    Math.max(segment.outPoint - segment.inPoint, 0) / audioSegmentPlaybackRate(segment.playbackRate);
  let cursor = existing.reduce((end, segment) => Math.max(end, segment.startTime + duration(segment)), 0);
  const placed = incoming.map((segment) => {
    const next = { ...segment, startTime: cursor };
    cursor += duration(segment);
    return next;
  });
  return [...existing, ...placed];
}
