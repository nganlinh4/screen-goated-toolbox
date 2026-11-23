import { useMemo } from "react";
import type { ProjectComposition } from "@/types/video";
import {
  buildSequenceTimeline,
  clipViewTimeToSequenceTime,
  getSequenceClipById,
  sequenceTimeToClipViewTime,
} from "@/lib/sequenceTimeline";

/**
 * Maps between project (sequence) time and the loaded clip's source-time
 * editor. Project-wide audio (imported music, narration) lives in sequence
 * time: the clips' kept ranges laid end to end, as export maps it. The editor
 * plays and draws one clip at a time on that clip's source clock.
 */
export interface ClipAudioView {
  /** Sequence time -> the loaded clip's source-time timeline. */
  toView: (sequenceTime: number) => number;
  /** The loaded clip's source-time timeline -> sequence time. */
  toSequence: (viewTime: number) => number;
}

/** The view for the loaded clip of a multi-clip project; null for single-clip projects (they stay in source time). */
export function useClipAudioView(
  composition: ProjectComposition | null | undefined,
  loadedClipId: string | null | undefined,
): ClipAudioView | null {
  const clips = composition?.clips;
  const timeline = useMemo(
    () => (composition && (clips?.length ?? 0) > 1 ? buildSequenceTimeline(composition) : null),
    // Rebuild only when the clips (and their trims) change.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [clips],
  );
  // Same active-clip choice as useSequenceComposition: the loaded clip, else the focused/selected one.
  const activeClip = getSequenceClipById(
    timeline,
    loadedClipId ?? composition?.focusedClipId ?? composition?.selectedClipId,
  );
  return useMemo(
    () => activeClip && {
      toView: (time: number) => sequenceTimeToClipViewTime(time, activeClip),
      toSequence: (time: number) => clipViewTimeToSequenceTime(time, activeClip),
    },
    [activeClip],
  );
}

/** The clock project audio plays against in the preview: sequence time for multi-clip projects. */
export function projectAudioClock(view: ClipAudioView | null, currentTime: number): number {
  return view ? view.toSequence(currentTime) : currentTime;
}
