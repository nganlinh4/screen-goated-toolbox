import { useMemo } from "react";
import type { AudioGainPoint } from "@/types/video";
import type { ClipAudioView } from "@/hooks/useClipAudioView";

type TimedAudioSegment = {
  id: string;
  startTime: number;
  inPoint: number;
  outPoint: number;
  playbackRate?: number;
};

function timelineLength(segment: TimedAudioSegment) {
  const rate = segment.playbackRate && segment.playbackRate > 0 ? segment.playbackRate : 1;
  return Math.max(0, segment.outPoint - segment.inPoint) / rate;
}

/**
 * Project audio segments placed on the loaded clip's timeline: each start is
 * mapped from sequence time, and segments that do not reach into the clip's
 * [0, duration] view are left out (they play under other clips).
 */
export function audioSegmentsInClipView<T extends TimedAudioSegment>(
  segments: readonly T[],
  view: ClipAudioView,
  duration: number,
): T[] {
  return segments.flatMap((segment) => {
    const startTime = view.toView(segment.startTime);
    const endTime = startTime + timelineLength(segment);
    return endTime > 0 && startTime < duration ? [{ ...segment, startTime }] : [];
  });
}

/** A clip-view edit patch back in sequence time (only startTime is a timeline position). */
export function audioPatchToSequence<P extends { startTime?: number }>(patch: P, view: ClipAudioView): P {
  return patch.startTime === undefined ? patch : { ...patch, startTime: view.toSequence(patch.startTime) };
}

const mapPointTimes = (points: AudioGainPoint[], map: (time: number) => number) =>
  points.map((point) => ({ ...point, time: map(point.time) }));

interface AudioTrackTimeViewOptions<T extends TimedAudioSegment> {
  /** Null for single-clip projects: everything passes through unchanged. */
  view: ClipAudioView | null | undefined;
  segments: T[];
  duration: number;
  onUpdateSegment?: (id: string, patch: Partial<T>) => void;
  volumePoints?: AudioGainPoint[];
  onUpdateVolumePoints?: (points: AudioGainPoint[]) => void;
}

/**
 * Shows a project audio lane (imported music or narration) on the loaded
 * clip's timeline and maps its edits back to sequence time, so the timeline,
 * the preview (usePreviewAudioClock path) and export agree.
 */
export function useAudioTrackTimeView<T extends TimedAudioSegment>({
  view,
  segments,
  duration,
  onUpdateSegment,
  volumePoints,
  onUpdateVolumePoints,
}: AudioTrackTimeViewOptions<T>) {
  return useMemo(() => {
    if (!view) return { segments, onUpdateSegment, volumePoints, onUpdateVolumePoints };
    return {
      segments: audioSegmentsInClipView(segments, view, duration),
      onUpdateSegment: onUpdateSegment
        && ((id: string, patch: Partial<T>) => onUpdateSegment(id, audioPatchToSequence(patch, view))),
      volumePoints: volumePoints && mapPointTimes(volumePoints, view.toView),
      onUpdateVolumePoints: onUpdateVolumePoints
        && ((points: AudioGainPoint[]) => onUpdateVolumePoints(mapPointTimes(points, view.toSequence))),
    };
  }, [view, segments, duration, onUpdateSegment, volumePoints, onUpdateVolumePoints]);
}
