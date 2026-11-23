import { renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { projectAudioClock, useClipAudioView } from "@/hooks/useClipAudioView";
import type { ProjectComposition, ProjectCompositionClip, VideoSegment } from "@/types/video";

function clip(id: string, duration: number, trimSegments: Array<[number, number]>): ProjectCompositionClip {
  const segment = {
    trimStart: trimSegments[0][0],
    trimEnd: trimSegments[trimSegments.length - 1][1],
    trimSegments: trimSegments.map(([startTime, endTime], i) => ({ id: `${id}-t${i}`, startTime, endTime })),
  } as VideoSegment;
  return { id, duration, segment } as unknown as ProjectCompositionClip;
}

function composition(clips: ProjectCompositionClip[]): ProjectComposition {
  return { clips, selectedClipId: clips[0]?.id ?? null, focusedClipId: null } as unknown as ProjectComposition;
}

/** The preview's project-audio clock, exactly as EditorMain derives it. */
function usePreviewAudioClock(project: ProjectComposition | null, clipId: string | null, currentTime: number) {
  return projectAudioClock(useClipAudioView(project, clipId), currentTime);
}

describe("preview project-audio clock", () => {
  // Clip A keeps 2-6 s (4 s); clip B keeps 1-3 s and 5-9 s (6 s) and follows A in the sequence.
  const project = composition([clip("a", 10, [[2, 6]]), clip("b", 10, [[1, 3], [5, 9]])]);

  it("plays project music and narration on the sequence clock of the loaded clip", () => {
    // Clip A at source 3 s is 1 s into the sequence.
    expect(renderHook(() => usePreviewAudioClock(project, "a", 3)).result.current).toBe(1);
    // Clip B at source 6 s: A's 4 s, plus B's first kept range (2 s), plus 1 s into the second.
    expect(renderHook(() => usePreviewAudioClock(project, "b", 6)).result.current).toBe(7);
  });

  it("does not jump across a cut inside a clip", () => {
    const beforeCut = renderHook(() => usePreviewAudioClock(project, "b", 2.99)).result.current;
    const afterCut = renderHook(() => usePreviewAudioClock(project, "b", 5)).result.current;
    expect(afterCut - beforeCut).toBeCloseTo(0.01);
  });

  it("follows the focused clip before a clip id is loaded", () => {
    const focused = { ...project, focusedClipId: "b" };
    expect(renderHook(() => usePreviewAudioClock(focused, null, 2)).result.current).toBe(5);
  });

  it("keeps single-clip projects on the source clock", () => {
    const single = composition([clip("only", 10, [[2, 6]])]);
    expect(renderHook(() => usePreviewAudioClock(single, "only", 3)).result.current).toBe(3);
    expect(renderHook(() => usePreviewAudioClock(null, null, 3)).result.current).toBe(3);
  });
});
