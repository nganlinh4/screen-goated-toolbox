import type { ProjectComposition, SubtitleSegment } from '@/types/video';
import { getSubtitleTracks } from '@/lib/subtitleTracks';
import { isSequenceAudioSubtitle } from '@/lib/compositionSubtitles';
import { buildSequenceTimeline, clipSourceTimeToSequenceTime, getSequenceClipById } from '@/lib/sequenceTimeline';

/** Narration is project-wide; selection happens in the editor's local clock first. */
export function projectSubtitleForNarration(
  composition: ProjectComposition | null,
  clipId: string | null,
  subtitle: SubtitleSegment,
): SubtitleSegment | null {
  if (!composition) return subtitle;
  const canonical = getSubtitleTracks(composition.globalSegment).flatMap((track) => track.segments)
    .find((cue) => cue.id === subtitle.id && isSequenceAudioSubtitle(cue));
  if (canonical) return { ...subtitle, startTime: canonical.startTime, endTime: canonical.endTime,
    splitGroupStartTime: canonical.splitGroupStartTime, splitGroupEndTime: canonical.splitGroupEndTime,
  };
  const timeline = buildSequenceTimeline(composition);
  const clip = timeline && getSequenceClipById(timeline,
    clipId ?? (composition.clips.length === 1 ? composition.clips[0].id : null));
  if (!clip) return null;
  const time = (value: number) => clipSourceTimeToSequenceTime(value, clip);
  return { ...subtitle, startTime: time(subtitle.startTime), endTime: time(subtitle.endTime),
    splitGroupStartTime: subtitle.splitGroupStartTime === undefined ? undefined : time(subtitle.splitGroupStartTime),
    splitGroupEndTime: subtitle.splitGroupEndTime === undefined ? undefined : time(subtitle.splitGroupEndTime),
  };
}
