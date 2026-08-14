import { describe, expect, it } from "vitest";
import {
  buildExportSourceTimes,
  getExportSourceStep,
} from "@/lib/exportFrameTimes";
import {
  DEFAULT_WEBCAM_CONFIG,
  buildBakedWebcamFrames,
} from "@/lib/webcam";
import type { VideoSegment } from "@/types/video";

function segment(overrides: Partial<VideoSegment> = {}): VideoSegment {
  return {
    trimStart: 0,
    trimEnd: 1,
    trimSegments: [{ id: "trim", startTime: 0, endTime: 1 }],
    speedPoints: [{ time: 0, speed: 1 }, { time: 1, speed: 1 }],
    zoomKeyframes: [],
    textSegments: [],
    subtitleSegments: [],
    ...overrides,
  };
}

describe("export source frame timing", () => {
  it("uses the native-exclusive trim end and speed-adjusted source step", () => {
    const normal = buildExportSourceTimes(segment(), 60);
    const fastSegment = segment({
      speedPoints: [{ time: 0, speed: 2 }, { time: 1, speed: 2 }],
    });
    const fast = buildExportSourceTimes(fastSegment, 60);

    expect(normal).toHaveLength(60);
    expect(fast).toHaveLength(30);
    expect(getExportSourceStep(fastSegment, 0.25, 60)).toBeCloseTo(2 / 60, 12);
  });

  it("jumps directly between disjoint trim segments", () => {
    const times = buildExportSourceTimes(segment({
      trimEnd: 1.5,
      trimSegments: [
        { id: "a", startTime: 0, endTime: 0.5 },
        { id: "b", startTime: 1, endTime: 1.5 },
      ],
      speedPoints: [{ time: 0, speed: 1 }, { time: 1.5, speed: 1 }],
    }), 60);

    expect(times.some((time) => time > 0.5 && time < 1)).toBe(false);
    expect(times.some((time) => Math.abs(time - 1) < 1e-9)).toBe(true);
  });

  it("bakes one webcam layout for each exact export source time", () => {
    const source = segment({
      speedPoints: [{ time: 0, speed: 2 }, { time: 1, speed: 2 }],
      webcamAvailable: true,
    });
    const expectedTimes = buildExportSourceTimes(source, 60);
    const sampledTimes: number[] = [];
    const frames = buildBakedWebcamFrames(
      source,
      { ...DEFAULT_WEBCAM_CONFIG, visible: true },
      1920,
      1080,
      16 / 9,
      (time) => {
        sampledTimes.push(time);
        return 1;
      },
      60,
    );

    expect(frames.map((frame) => frame.time)).toEqual(expectedTimes);
    expect(sampledTimes).toEqual(expectedTimes);
  });
});
