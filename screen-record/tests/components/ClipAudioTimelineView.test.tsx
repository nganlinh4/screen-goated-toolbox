import { render, renderHook, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { NarrationTrack } from "@/components/timeline/NarrationTrack";
import { ImportedAudioTrack } from "@/components/timeline/ImportedAudioTrack";
import { projectAudioClock, useClipAudioView, type ClipAudioView } from "@/hooks/useClipAudioView";
import en from "@/i18n/en";
import type {
  ImportedAudioSegment,
  NarrationSegment,
  ProjectComposition,
  ProjectCompositionClip,
  VideoSegment,
} from "@/types/video";

vi.mock("@/hooks/useSettings", () => ({ useSettings: () => ({ t: en, lang: "en" }) }));
vi.mock("@/components/timeline/AudioWaveformLayer", () => ({ AudioWaveformLayer: () => null }));

function clip(id: string, trims: Array<[number, number]>): ProjectCompositionClip {
  const segment = {
    trimStart: trims[0][0],
    trimEnd: trims[trims.length - 1][1],
    trimSegments: trims.map(([startTime, endTime], i) => ({ id: `${id}${i}`, startTime, endTime })),
  } as VideoSegment;
  return { id, duration: 10, segment } as unknown as ProjectCompositionClip;
}

// Clip A keeps 2-6 s (sequence 0-4); clip B keeps 1-3 s and 5-9 s (sequence 4-10).
const project = {
  clips: [clip("a", [[2, 6]]), clip("b", [[1, 3], [5, 9]])],
  selectedClipId: "a",
  focusedClipId: null,
} as unknown as ProjectComposition;

function viewFor(clipId: string): ClipAudioView {
  return renderHook(() => useClipAudioView(project, clipId)).result.current!;
}

const narration = (id: string, startTime: number) => ({
  id, rawAudioPath: `${id}.wav`, name: id, duration: 2, startTime, inPoint: 0, outPoint: 1, addedAt: 0,
}) as NarrationSegment;

describe("project audio on a multi-clip timeline", () => {
  it("maps sequence time onto the loaded clip's timeline and back", () => {
    const b = viewFor("b");
    expect(b.toView(7)).toBe(6);
    expect(b.toSequence(6)).toBe(7);
    // Before the clip the offset continues from its first kept edge.
    expect(b.toView(3)).toBe(0);
    expect(b.toSequence(0)).toBe(3);
    // The playhead clock and the timeline agree.
    expect(projectAudioClock(b, 6)).toBe(b.toSequence(6));
  });

  it("draws narration where the preview plays it and writes moves back in sequence time", async () => {
    const onUpdateSegment = vi.fn();
    render(
      <NarrationTrack
        segments={[narration("here", 7), narration("other-clip", 1)]}
        duration={10}
        timeView={viewFor("b")}
        onUpdateSegment={onUpdateSegment}
      />,
    );
    const shown = document.querySelectorAll<HTMLElement>("[data-narration-segment-id]");
    expect([...shown].map((element) => element.dataset.narrationSegmentId)).toEqual(["here"]);
    expect(shown[0].style.left).toBe("60%");

    const user = userEvent.setup();
    screen.getByRole("button", { name: /^Narration "here", 6\.00 s/ }).focus();
    await user.keyboard("{ArrowRight}");
    expect(onUpdateSegment.mock.lastCall![0]).toBe("here");
    expect(onUpdateSegment.mock.lastCall![1].startTime).toBeCloseTo(7.1);
  });

  it("maps imported music and its volume envelope the same way", async () => {
    const music = { ...narration("music", 5), name: "music" } as ImportedAudioSegment;
    const onUpdateVolumePoints = vi.fn();
    render(
      <ImportedAudioTrack
        segments={[music]}
        duration={10}
        timeView={viewFor("b")}
        viewMode="volume"
        volumePoints={[{ time: 4, volume: 1 }, { time: 10, volume: 0.5 }]}
        onUpdateVolumePoints={onUpdateVolumePoints}
      />,
    );
    const shown = document.querySelector<HTMLElement>("[data-audio-segment-id='music']")!;
    // Sequence 5 s is 1 s into clip B: source 2 s.
    expect(shown.style.left).toBe("20%");
    const user = userEvent.setup();
    screen.getByRole("slider", { name: "Volume point 2" }).focus();
    await user.keyboard("{ArrowUp}");
    // The edited envelope is written back in sequence time.
    expect(onUpdateVolumePoints.mock.lastCall![0].map((point: { time: number }) => point.time)).toEqual([4, 10]);
  });

  it("leaves single-clip projects in source time", () => {
    const single = { ...project, clips: [project.clips[0]] } as ProjectComposition;
    expect(renderHook(() => useClipAudioView(single, "a")).result.current).toBeNull();
    render(<NarrationTrack segments={[narration("n", 7)]} duration={10} timeView={null} />);
    expect(document.querySelector<HTMLElement>("[data-narration-segment-id='n']")!.style.left).toBe("70%");
  });
});
