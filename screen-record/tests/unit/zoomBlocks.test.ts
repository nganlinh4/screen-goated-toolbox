import { describe, expect, it } from "vitest";
import {
  calculateCurrentZoomStateInternal,
  zoomBlockEnvelope,
  zoomKeyframesToBlocks,
} from "@/lib/renderer/cameraZoom";
import type { VideoSegment, ZoomBlock } from "@/types/video";

const VIEW = 1000;

// Minimal segment carrying only the fields the resolver reads.
function makeSegment(overrides: Partial<VideoSegment>): VideoSegment {
  return {
    trimStart: 0,
    trimEnd: 10,
    zoomKeyframes: [],
    textSegments: [],
    // Constant auto path: zoom 2.0, centered, across the whole clip.
    smoothMotionPath: [
      { time: 0, x: VIEW / 2, y: VIEW / 2, zoom: 2.0 },
      { time: 10, x: VIEW / 2, y: VIEW / 2, zoom: 2.0 },
    ],
    ...overrides,
  } as VideoSegment;
}

const block = (over: Partial<ZoomBlock>): ZoomBlock => ({
  id: Math.random().toString(36),
  startTime: 0,
  endTime: 1,
  easeIn: 0.4,
  easeOut: 0.4,
  zoomFactor: 1.5,
  positionX: 0.5,
  positionY: 0.5,
  followCursor: false,
  enabled: true,
  ...over,
});

describe("zoom block resolver", () => {
  // The regression this whole change exists to fix: a manual zoom on the left
  // AND on the right must NOT suppress auto-zoom in the gap between them.
  it("reverts to the auto path in the gap between two blocks", () => {
    const segment = makeSegment({
      zoomBlocks: [
        block({ startTime: 1, endTime: 3, zoomFactor: 1.5, positionX: 0.2, positionY: 0.2 }),
        block({ startTime: 6, endTime: 8, zoomFactor: 1.8, positionX: 0.8, positionY: 0.8 }),
      ],
    });

    // Inside block A's hold → pure manual.
    const inA = calculateCurrentZoomStateInternal(2, segment, VIEW, VIEW);
    expect(inA.zoomFactor).toBeCloseTo(1.5, 3);
    expect(inA.positionX).toBeCloseTo(0.2, 3);

    // Mid-gap → auto path (zoom 2.0, centered), NOT a manual interpolation.
    const inGap = calculateCurrentZoomStateInternal(4.5, segment, VIEW, VIEW);
    expect(inGap.zoomFactor).toBeCloseTo(2.0, 3);
    expect(inGap.positionX).toBeCloseTo(0.5, 3);

    // Inside block B's hold → pure manual.
    const inB = calculateCurrentZoomStateInternal(7, segment, VIEW, VIEW);
    expect(inB.zoomFactor).toBeCloseTo(1.8, 3);
    expect(inB.positionX).toBeCloseTo(0.8, 3);
  });

  it("returns the auto path before the first block and after the last", () => {
    const segment = makeSegment({
      zoomBlocks: [block({ startTime: 4, endTime: 6, zoomFactor: 1.5 })],
    });
    expect(calculateCurrentZoomStateInternal(1, segment, VIEW, VIEW).zoomFactor).toBeCloseTo(2.0, 3);
    expect(calculateCurrentZoomStateInternal(9, segment, VIEW, VIEW).zoomFactor).toBeCloseTo(2.0, 3);
  });

  it("ignores disabled blocks", () => {
    const segment = makeSegment({
      zoomBlocks: [block({ startTime: 2, endTime: 5, zoomFactor: 1.5, enabled: false })],
    });
    expect(calculateCurrentZoomStateInternal(3.5, segment, VIEW, VIEW).zoomFactor).toBeCloseTo(2.0, 3);
  });
});

describe("zoomBlockEnvelope", () => {
  const b = block({ startTime: 2, endTime: 8, easeIn: 1, easeOut: 1 });
  it("is 0 outside the block, 1 across the hold, eased on the ramps", () => {
    expect(zoomBlockEnvelope(b, 1.9)).toBe(0);
    expect(zoomBlockEnvelope(b, 8.1)).toBe(0);
    expect(zoomBlockEnvelope(b, 5)).toBe(1); // hold
    expect(zoomBlockEnvelope(b, 2.5)).toBeGreaterThan(0);
    expect(zoomBlockEnvelope(b, 2.5)).toBeLessThan(1);
  });
});

describe("zoomKeyframesToBlocks migration", () => {
  it("converts each keyframe into a bounded block", () => {
    const blocks = zoomKeyframesToBlocks(
      [
        { time: 2, duration: 0, zoomFactor: 1.5, positionX: 0.3, positionY: 0.3, easingType: "easeInOut" },
        { time: 8, duration: 0, zoomFactor: 2.0, positionX: 0.7, positionY: 0.7, easingType: "easeInOut" },
      ],
      10,
    );
    expect(blocks).toHaveLength(2);
    expect(blocks[0].zoomFactor).toBe(1.5);
    expect(blocks[0].startTime).toBeLessThan(2);
    expect(blocks[0].endTime).toBeGreaterThan(2);
    expect(blocks[1].endTime).toBeLessThanOrEqual(10);
  });

  it("returns an empty list for no keyframes", () => {
    expect(zoomKeyframesToBlocks([], 10)).toEqual([]);
  });
});
