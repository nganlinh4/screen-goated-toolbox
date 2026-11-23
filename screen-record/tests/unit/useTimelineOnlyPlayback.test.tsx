import { renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  TIMELINE_ONLY_STATE_COMMIT_MS,
  useTimelineOnlyPlayback,
} from "@/hooks/useTimelineOnlyPlayback";
import { readTimelinePlaybackTime } from "@/lib/timelinePlaybackClock";
import type { VideoSegment } from "@/types/video";

const FRAME_MS = 1000 / 60;

describe("useTimelineOnlyPlayback", () => {
  let now = 0;
  let frames: Array<FrameRequestCallback | null> = [];

  const runFrames = (count: number) => {
    for (let i = 0; i < count; i += 1) {
      now += FRAME_MS;
      const pending = frames;
      frames = [];
      pending.forEach((cb) => cb?.(now));
    }
  };

  beforeEach(() => {
    now = 1000;
    frames = [];
    vi.spyOn(performance, "now").mockImplementation(() => now);
    vi.stubGlobal("requestAnimationFrame", (cb: FrameRequestCallback) => {
      frames.push(cb);
      return frames.length;
    });
    vi.stubGlobal("cancelAnimationFrame", (id: number) => {
      frames[id - 1] = null;
    });
  });

  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  const segment = { trimStart: 0, trimEnd: 10, speedPoints: [] } as unknown as VideoSegment;

  it("renders every frame but commits React time state at a throttled cadence", () => {
    const setCurrentTime = vi.fn();
    const renderFrame = vi.fn();
    const currentTimeRef = { current: 0 };
    const { rerender } = renderHook(
      ({ isPlaying }) =>
        useTimelineOnlyPlayback({
          isTimelineOnly: true,
          isPlaying,
          segment,
          duration: 10,
          currentTimeRef,
          setCurrentTime,
          setIsPlaying: vi.fn(),
          renderFrame,
        }),
      { initialProps: { isPlaying: true } },
    );

    runFrames(60);
    expect(renderFrame).toHaveBeenCalledTimes(60);
    expect(readTimelinePlaybackTime()).toBeCloseTo(1, 2);
    const expectedCommits = Math.floor(1000 / TIMELINE_ONLY_STATE_COMMIT_MS);
    expect(setCurrentTime.mock.calls.length).toBeLessThanOrEqual(expectedCommits);
    expect(setCurrentTime.mock.calls.length).toBeGreaterThan(0);

    rerender({ isPlaying: false });
    expect(setCurrentTime).toHaveBeenLastCalledWith(currentTimeRef.current);
  });

  it("commits the end time and stops when playback reaches the end", () => {
    const setCurrentTime = vi.fn();
    const setIsPlaying = vi.fn();
    const currentTimeRef = { current: 9.95 };
    renderHook(() =>
      useTimelineOnlyPlayback({
        isTimelineOnly: true,
        isPlaying: true,
        segment,
        duration: 10,
        currentTimeRef,
        setCurrentTime,
        setIsPlaying,
        renderFrame: vi.fn(),
      }),
    );

    runFrames(10);
    expect(setCurrentTime).toHaveBeenLastCalledWith(10);
    expect(setIsPlaying).toHaveBeenCalledWith(false);
    expect(frames.filter(Boolean)).toHaveLength(0);
  });
});
