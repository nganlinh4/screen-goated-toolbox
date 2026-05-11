import { describe, expect, it } from "vitest";
import {
  calculateOutputDuration,
  computeBitrateSliderBounds,
  computeGifResolutionOptions,
  computeResolutionOptions,
  estimateExportSize,
  resolveExportDimensions,
  videoTimeToWallClock,
} from "@/lib/exportEstimator";
import {
  clampToTrimSegments,
  getNextPlayableTime,
  getTotalTrimDuration,
  normalizeSegmentTrimData,
  normalizeSpeedPoints,
  sourceRangeToCompactRanges,
  toCompactTime,
  toSourceTime,
} from "@/lib/trimSegments";
import type { VideoSegment } from "@/types/video";

function segment(overrides: Partial<VideoSegment> = {}): VideoSegment {
  return {
    trimStart: 0,
    trimEnd: 12,
    trimSegments: [
      { id: "a", startTime: 2, endTime: 5 },
      { id: "b", startTime: 8, endTime: 12 },
    ],
    zoomKeyframes: [],
    textSegments: [],
    speedPoints: [
      { time: 0, speed: 1 },
      { time: 12, speed: 1 },
    ],
    ...overrides,
  };
}

describe("timeline trim and speed math", () => {
  it("maps source time through disjoint trim segments without including gaps", () => {
    const videoSegment = segment();

    expect(getTotalTrimDuration(videoSegment, 12)).toBe(7);
    expect(toCompactTime(4, videoSegment, 12)).toBe(2);
    expect(toCompactTime(9, videoSegment, 12)).toBe(4);
    expect(toSourceTime(4, videoSegment, 12)).toBe(9);
    expect(sourceRangeToCompactRanges(4, 10, videoSegment, 12)).toEqual([
      { start: 2, end: 3 },
      { start: 3, end: 5 },
    ]);
  });

  it("clamps playback probes to the closest playable trim boundary", () => {
    const videoSegment = segment();

    expect(clampToTrimSegments(1, videoSegment, 12)).toBe(2);
    expect(clampToTrimSegments(5.9, videoSegment, 12)).toBe(5);
    expect(clampToTrimSegments(7.2, videoSegment, 12)).toBe(8);
    expect(getNextPlayableTime(5.1, videoSegment, 12)).toBe(8);
    expect(getNextPlayableTime(12.01, videoSegment, 12)).toBeNull();
  });

  it("normalizes invalid speed points and keeps endpoint coverage", () => {
    const points = normalizeSpeedPoints([
      { time: -5, speed: 20 },
      { time: 5, speed: 0 },
      { time: 5.00001, speed: 2 },
      { time: Number.NaN, speed: 1 },
      { time: 20, speed: 3 },
    ], 10);

    expect(points[0]).toEqual({ time: 0, speed: 16 });
    expect(points[1]).toEqual({ time: 5.00001, speed: 2 });
    expect(points[2]).toEqual({ time: 10, speed: 3 });
  });

  it("normalizes trim state before sending native export jobs", () => {
    const normalized = normalizeSegmentTrimData(segment({
      trimStart: -10,
      trimEnd: 99,
      trimSegments: [
        { id: "drop", startTime: 2, endTime: 2.04 },
        { id: "keep", startTime: 3, endTime: 6 },
      ],
      speedPoints: [{ time: 100, speed: 12 }],
    }), 10);

    expect(normalized.trimStart).toBe(3);
    expect(normalized.trimEnd).toBe(6);
    expect(normalized.trimSegments).toEqual([{ id: "keep", startTime: 3, endTime: 6 }]);
    expect(normalized.speedPoints?.at(-1)).toEqual({ time: 6, speed: 12 });
  });
});

describe("export estimator behavior", () => {
  it("keeps MP4 dimensions even and preserves original resolution when 0x0 is requested", () => {
    expect(resolveExportDimensions(0, 0, 1253, 947)).toEqual({ width: 1252, height: 946 });
    expect(resolveExportDimensions(961, 725, 1920, 1080)).toEqual({ width: 960, height: 724 });

    const options = computeResolutionOptions(1253, 947, 1080);
    expect(options[0]).toEqual({ width: 1252, height: 946, label: "Original (1252 × 946)" });
  });

  it("caps GIF dimensions by width while preserving aspect ratio", () => {
    const options = computeGifResolutionOptions(2560, 1080);
    expect(options[0]).toEqual({ width: 960, height: 404, label: "960w" });
    expect(options.every((option) => option.width <= 960 && option.width % 2 === 0)).toBe(true);
  });

  it("integrates speed curves into output duration and wall-clock time", () => {
    const fast = segment({
      trimSegments: [{ id: "full", startTime: 0, endTime: 10 }],
      speedPoints: [
        { time: 0, speed: 2 },
        { time: 10, speed: 2 },
      ],
    });

    expect(calculateOutputDuration(fast, 10)).toBeCloseTo(5, 2);
    expect(videoTimeToWallClock(10, fast.speedPoints ?? [])).toBeCloseTo(5, 2);
  });

  it("estimates GIF and MP4 with different bitrate models", () => {
    const mp4Bounds = computeBitrateSliderBounds(1920, 1080, 60);
    expect(mp4Bounds.recommendedKbps).toBeGreaterThan(mp4Bounds.minKbps);
    expect(mp4Bounds.maxKbps).toBeGreaterThan(mp4Bounds.recommendedKbps);

    const mp4Estimate = estimateExportSize({
      width: 1920,
      height: 1080,
      fps: 60,
      targetVideoBitrateKbps: 12_000,
      trimmedDurationSec: 10,
      hasAudio: true,
      calibration: { ratio: 1, samples: 0 },
    });
    const gifEstimate = estimateExportSize({
      width: 1920,
      height: 1080,
      fps: 15,
      format: "gif",
      targetVideoBitrateKbps: 12_000,
      trimmedDurationSec: 10,
    });

    expect(mp4Estimate.targetVideoBitrateKbps).toBe(12_000);
    expect(mp4Estimate.audioBitrateKbps).toBeGreaterThan(0);
    expect(gifEstimate.targetVideoBitrateKbps).toBe(0);
    expect(gifEstimate.profileKey).toBe("gif");
  });
});
