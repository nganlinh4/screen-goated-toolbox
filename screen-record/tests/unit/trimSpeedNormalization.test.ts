import { describe, expect, it } from "vitest";
import { normalizeSegmentTrimData } from "@/lib/trimSegments";
import { getSpeedAtTime } from "@/lib/speedCurve";
import type { VideoSegment } from "@/types/video";

function segment(overrides: Partial<VideoSegment>): VideoSegment {
  return {
    trimStart: 0,
    trimEnd: 7,
    trimSegments: [{ id: "t", startTime: 0, endTime: 7 }],
    zoomKeyframes: [],
    textSegments: [],
    ...overrides,
  } as VideoSegment;
}

describe("normalizeSegmentTrimData speed curve", () => {
  // The speed track spans the whole source, so a ramp can start inside the
  // kept range and peak past the trim end.
  const speedPoints = [
    { time: 0, speed: 1 },
    { time: 8, speed: 3 },
    { time: 10, speed: 1 },
  ];

  it("keeps preview speeds inside the kept range unchanged (export and reload use this)", () => {
    const raw = segment({ speedPoints });
    const normalized = normalizeSegmentTrimData(raw, 10);
    for (const time of [1, 4, 6.5, 6.99]) {
      expect(getSpeedAtTime(time, normalized.speedPoints)).toBeCloseTo(getSpeedAtTime(time, raw.speedPoints), 9);
    }
  });

  it("keeps speed edits past the trim end so widening the trim restores them", () => {
    const normalized = normalizeSegmentTrimData(segment({ speedPoints }), 10);
    expect(normalized.speedPoints).toEqual(speedPoints);
  });
});
