import { act, render } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ImportedAudioPlayers } from "@/components/ImportedAudioPlayers";
import { getSpeedAtTime } from "@/lib/speedCurve";
import type { ImportedAudioSegment, SpeedPoint } from "@/types/video";

vi.mock("@/lib/mediaServer", () => ({
  getMediaServerUrl: vi.fn(async (path: string) => `http://media.test/${path}`),
}));
vi.mock("@/lib/liveNarrationStreamStore", () => ({
  useLiveNarrationState: () => null,
  mergeLiveNarrationSegments: (segments: unknown[]) => segments,
}));

const music = {
  id: "m1", rawAudioPath: "music.wav", name: "Music", duration: 60,
  startTime: 0, inPoint: 0, outPoint: 60, addedAt: 0,
} as ImportedAudioSegment;

// Slow on the clip's source clock up to 4 s, fast after it.
const speedPoints: SpeedPoint[] = [
  { time: 0, speed: 1 }, { time: 4, speed: 1 }, { time: 4.5, speed: 2 }, { time: 20, speed: 2 },
];

describe("ImportedAudioPlayers clocks", () => {
  it("places project audio on the project clock but samples the clip's speed curve on the clip clock", async () => {
    // Sequence time 1 s while the loaded clip sits at source time 6 s (a trimmed or later clip).
    render(
      <ImportedAudioPlayers
        segments={[music]} speedPoints={speedPoints}
        currentTime={1} speedCurveTime={6} isPlaying={false}
      />,
    );
    await act(async () => {});
    const audio = document.querySelector<HTMLAudioElement>("audio.imported-audio-element")!;
    expect(audio).not.toBeNull();
    expect(audio.playbackRate).toBeCloseTo(getSpeedAtTime(6, speedPoints));
    expect(audio.playbackRate).not.toBeCloseTo(getSpeedAtTime(1, speedPoints));
  });

  it("uses one clock for both when no clip clock is given (single-clip projects)", async () => {
    render(<ImportedAudioPlayers segments={[music]} speedPoints={speedPoints} currentTime={6} isPlaying={false} />);
    await act(async () => {});
    const audio = document.querySelector<HTMLAudioElement>("audio.imported-audio-element")!;
    expect(audio.playbackRate).toBeCloseTo(getSpeedAtTime(6, speedPoints));
  });
});
