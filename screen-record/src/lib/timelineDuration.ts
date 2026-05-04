import type {
  ImportedAudioSegment,
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

export function getImportedAudioEnd(audioSegments: readonly ImportedAudioSegment[] | undefined) {
  return Math.max(
    0,
    ...(audioSegments ?? []).map((segment) => {
      const visibleDuration = Math.max(segment.outPoint - segment.inPoint, 0);
      return segment.startTime + visibleDuration;
    }),
  );
}

export function getTimelineContentEnd(
  segment: VideoSegment | null | undefined,
  audioSegments?: readonly ImportedAudioSegment[],
) {
  if (!segment) return Math.max(1, getImportedAudioEnd(audioSegments));
  return Math.max(
    1,
    maxSegmentEnd(segment.textSegments),
    maxSegmentEnd(getVisibleSubtitleSegments(segment)),
    getImportedAudioEnd(audioSegments),
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
