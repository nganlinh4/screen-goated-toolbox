import type { SubtitleSourceSnapshot } from '@/hooks/subtitleSourceOwnership';
import { updateAllSubtitleTracks } from '@/lib/subtitleTracks';
import type { VideoSegment } from '@/types/video';

export function narrationEditorOwner(snapshot: SubtitleSourceSnapshot): string {
  const clip = snapshot.composition?.clips.find((item) => item.id === snapshot.loadedClipId);
  const segment = snapshot.segment;
  return JSON.stringify([snapshot.loadedClipId, snapshot.videoPath, snapshot.micPath,
    snapshot.composition?.mode, clip?.rawVideoPath, clip?.duration, segment?.trimStart, segment?.trimEnd,
    segment?.trimSegments?.map(({ startTime, endTime }) => [startTime, endTime]), segment?.speedPoints,
    snapshot.composition?.clips.map((item) => [item.id, item.rawVideoPath, item.duration,
      item.segment.trimStart, item.segment.trimEnd,
      item.segment.trimSegments?.map(({ startTime, endTime }) => [startTime, endTime]), item.segment.speedPoints])]);
}

export function alignSubtitleTracksToNarrationTiming(
  segment: VideoSegment,
  timingBySubtitleId: ReadonlyMap<string, { startTime: number; endTime: number }>,
): VideoSegment {
  if (timingBySubtitleId.size === 0) return segment;
  return updateAllSubtitleTracks(segment, (track) => {
    const changedGroups = new Set<string>();
    const segments = track.segments.map((subtitle) => {
      const timing = timingBySubtitleId.get(subtitle.id);
      if (!timing) return subtitle;
      if (subtitle.splitGroupId) changedGroups.add(subtitle.splitGroupId);
      return { ...subtitle, startTime: timing.startTime, endTime: timing.endTime };
    });
    const bounds = new Map<string, { start: number; end: number }>();
    for (const subtitle of segments) {
      if (!subtitle.splitGroupId || !changedGroups.has(subtitle.splitGroupId)
        || !Number.isFinite(subtitle.startTime) || !Number.isFinite(subtitle.endTime)) continue;
      const prior = bounds.get(subtitle.splitGroupId);
      bounds.set(subtitle.splitGroupId, {
        start: Math.min(prior?.start ?? subtitle.startTime, subtitle.startTime),
        end: Math.max(prior?.end ?? subtitle.endTime, subtitle.endTime),
      });
    }
    return { ...track, segments: segments.map((subtitle) => {
      const group = subtitle.splitGroupId && bounds.get(subtitle.splitGroupId);
      return group ? { ...subtitle, splitGroupStartTime: group.start, splitGroupEndTime: group.end } : subtitle;
    }) };
  });
}
