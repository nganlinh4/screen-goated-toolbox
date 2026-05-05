import type {
  ImportedAudioSegment,
  NarrationSegment,
  ProjectComposition,
  SubtitleSegment,
  TextSegment,
  VideoSegment,
} from "@/types/video";
import { getVisibleSubtitleSegments } from "@/lib/subtitleTracks";

function maxSegmentEnd(segments: readonly TextSegment[] | readonly SubtitleSegment[] | undefined) {
  return Math.max(
    0,
    ...(segments ?? [])
      .map((segment) => segment.endTime)
      .filter((time) => Number.isFinite(time)),
  );
}

interface AudioLikeSegment {
  startTime: number;
  inPoint: number;
  outPoint: number;
  playbackRate?: number;
}

function audioLikeEnd(segments: readonly AudioLikeSegment[] | undefined) {
  return Math.max(
    0,
    ...(segments ?? []).map((segment) => {
      const trimmed = Math.max(segment.outPoint - segment.inPoint, 0);
      const rate = segment.playbackRate && segment.playbackRate > 0 ? segment.playbackRate : 1;
      const visibleDuration = trimmed / rate;
      return segment.startTime + visibleDuration;
    }),
  );
}

export function getImportedAudioEnd(audioSegments: readonly ImportedAudioSegment[] | undefined) {
  return audioLikeEnd(audioSegments);
}

export function getNarrationEnd(narrationSegments: readonly NarrationSegment[] | undefined) {
  return audioLikeEnd(narrationSegments);
}

export function getTimelineContentEnd(
  segment: VideoSegment | null | undefined,
  audioSegments?: readonly ImportedAudioSegment[],
  narrationSegments?: readonly NarrationSegment[],
) {
  const audioEnd = getImportedAudioEnd(audioSegments);
  const narrationEnd = getNarrationEnd(narrationSegments);
  if (!segment) return Math.max(1, audioEnd, narrationEnd);
  return Math.max(
    1,
    maxSegmentEnd(segment.textSegments),
    maxSegmentEnd(getVisibleSubtitleSegments(segment)),
    audioEnd,
    narrationEnd,
  );
}

export function resizeSegmentDuration(segment: VideoSegment, duration: number): VideoSegment {
  const safeDuration = Math.max(duration, 1);
  return {
    ...segment,
    trimStart: 0,
    trimEnd: safeDuration,
    trimSegments: [
      {
        id: segment.trimSegments?.[0]?.id ?? crypto.randomUUID(),
        startTime: 0,
        endTime: safeDuration,
      },
    ],
    speedPoints: [
      { time: 0, speed: 1 },
      { time: safeDuration, speed: 1 },
    ],
  };
}

export function resizeCompositionRootDuration(
  composition: ProjectComposition | null | undefined,
  segment: VideoSegment,
  duration: number,
): ProjectComposition | null {
  if (!composition) return null;
  const safeDuration = Math.max(duration, 1);
  return {
    ...composition,
    clips: composition.clips.map((clip) =>
      clip.id === "root"
        ? {
            ...clip,
            duration: safeDuration,
            segment,
          }
        : clip,
    ),
    globalSegment: composition.globalSegment ? segment : composition.globalSegment,
  };
}
