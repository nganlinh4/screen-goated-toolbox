import { renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useTimelineOnlyPlayback } from "@/hooks/useTimelineOnlyPlayback";
import type { VideoSegment } from "@/types/video";

const segment = { trimStart: 0, trimEnd: 10, speedPoints: [] } as unknown as VideoSegment;

describe("timeline-only playback", () => {
  beforeEach(() => {
    vi.useFakeTimers({ toFake: ["requestAnimationFrame", "cancelAnimationFrame", "performance"] });
  });
  afterEach(() => vi.useRealTimers());

  it("replays from the start when play is pressed at the end, like video projects", () => {
    const currentTimeRef = { current: 10 };
    const setIsPlaying = vi.fn();
    const setCurrentTime = vi.fn();
    const renderFrame = vi.fn();
    renderHook(() => useTimelineOnlyPlayback({
      isTimelineOnly: true, isPlaying: true, segment, duration: 10,
      currentTimeRef, setCurrentTime, setIsPlaying, renderFrame,
    }));
    vi.advanceTimersByTime(500);
    expect(setIsPlaying).not.toHaveBeenCalledWith(false);
    expect(currentTimeRef.current).toBeGreaterThan(0.3);
    expect(currentTimeRef.current).toBeLessThan(1);
  });

  it("still stops once it reaches the end", () => {
    const currentTimeRef = { current: 9.9 };
    const setIsPlaying = vi.fn();
    renderHook(() => useTimelineOnlyPlayback({
      isTimelineOnly: true, isPlaying: true, segment, duration: 10,
      currentTimeRef, setCurrentTime: vi.fn(), setIsPlaying, renderFrame: vi.fn(),
    }));
    vi.advanceTimersByTime(500);
    expect(setIsPlaying).toHaveBeenCalledWith(false);
    expect(currentTimeRef.current).toBe(10);
  });
});
