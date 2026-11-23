import type { ProjectComposition, SubtitleSegment, VideoSegment } from '@/types/video';
import { getSubtitleTracks, normalizeSubtitleTrackState } from '@/lib/subtitleTracks';
import { appendProjectedSubtitleTrackState } from '@/lib/sequenceSubtitleTracks';
import {
  buildSequenceTimeline,
  getSequenceClipById,
  mergeCompositionSegmentsToSequence,
  sequenceTimeToClipSourceTime,
} from '@/lib/sequenceTimeline';

export function isSequenceAudioSubtitle(subtitle: SubtitleSegment): boolean {
  return Boolean(subtitle.sourceGroup?.audioSegmentId ?? subtitle.provenance?.audioSegmentId);
}

function withProjectedAudioSubtitles(
  local: VideoSegment,
  global: VideoSegment | undefined,
  overlaps: (subtitle: SubtitleSegment) => boolean,
  projectTime: (time: number) => number,
): VideoSegment {
  if (!global) return local;
  const globalTracks = getSubtitleTracks(global).map((track) => ({
    ...track,
    segments: track.segments.filter(isSequenceAudioSubtitle),
  }));
  const globalIds = new Set(globalTracks.flatMap((track) => track.segments.map((cue) => cue.id)));
  if (globalIds.size === 0) return local;
  const base = normalizeSubtitleTrackState({ ...local,
    subtitleTracks: getSubtitleTracks(local).map((track) => ({ ...track,
      segments: track.segments.filter((cue) => !globalIds.has(cue.id)),
    })),
  });
  const projected = normalizeSubtitleTrackState({ ...global,
    subtitleTracks: globalTracks.map((track) => ({ ...track,
      segments: track.segments.filter(overlaps).map((cue) => ({ ...cue,
        startTime: projectTime(cue.startTime), endTime: projectTime(cue.endTime),
        splitGroupStartTime: cue.splitGroupStartTime === undefined ? undefined : projectTime(cue.splitGroupStartTime),
        splitGroupEndTime: cue.splitGroupEndTime === undefined ? undefined : projectTime(cue.splitGroupEndTime),
      })),
    })),
  });
  return normalizeSubtitleTrackState({ ...local,
    ...appendProjectedSubtitleTrackState(base, projected),
    activeSubtitleView: projected.activeSubtitleView,
    subtitleCustomChain: projected.subtitleCustomChain,
  });
}

/** A read-only clip-space view; sequence captions never enter the clip's saved segment. */
export function getCompositionClipRenderSegment(
  composition: ProjectComposition | null | undefined,
  clipId: string | null | undefined,
  segment: VideoSegment,
): VideoSegment {
  if (!composition?.globalSegment) return segment;
  const timeline = buildSequenceTimeline(composition);
  const clip = timeline && getSequenceClipById(timeline, clipId
    ?? (composition.clips.length === 1 ? composition.clips[0].id : null));
  if (!clip) return segment;
  return withProjectedAudioSubtitles(segment, composition.globalSegment,
    (cue) => cue.endTime > clip.sequenceStart && cue.startTime < clip.sequenceEnd,
    (time) => sequenceTimeToClipSourceTime(time, clip));
}

export function getCompositionSequenceRenderSegment(composition: ProjectComposition): VideoSegment | null {
  const timeline = buildSequenceTimeline(composition);
  if (!timeline) return null;
  const segment = mergeCompositionSegmentsToSequence(timeline);
  return withProjectedAudioSubtitles(segment, composition.globalSegment,
    (cue) => cue.endTime > 0 && cue.startTime < timeline.totalDuration,
    (time) => Math.min(timeline.totalDuration, Math.max(0, time)));
}
