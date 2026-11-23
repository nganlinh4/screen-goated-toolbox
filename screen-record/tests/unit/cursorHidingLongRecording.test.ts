import { describe, expect, it } from "vitest";
import { generateCursorVisibility } from "@/lib/cursorHiding";
import type { MousePosition, VideoSegment } from "@/types/video";

describe("cursor visibility for long recordings", () => {
  it("processes large sample arrays with explicit and inferred durations", () => {
    const positions: MousePosition[] = Array.from({ length: 200_000 }, (_, index) => ({
      x: index % 2 ? 0 : 200,
      y: 0,
      timestamp: index / 60,
    }));
    const segment: VideoSegment = {
      trimStart: 0,
      trimEnd: 1,
      zoomKeyframes: [],
      textSegments: [],
    };
    const duration = positions[positions.length - 1].timestamp;
    const explicit = generateCursorVisibility(segment, positions, duration);
    const inferred = generateCursorVisibility(segment, positions);
    expect(explicit.length).toBeGreaterThan(0);
    expect(inferred.map(({ startTime, endTime }) => ({ startTime, endTime })))
      .toEqual(explicit.map(({ startTime, endTime }) => ({ startTime, endTime })));
  }, 30_000);
});
