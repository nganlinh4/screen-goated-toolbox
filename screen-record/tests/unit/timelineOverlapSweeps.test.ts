import { describe, expect, it } from "vitest";
import { computeSegmentStackOverlaps } from "@/components/timeline/segmentStackOverlaps";
import {
  buildContentMaskStyle,
  getOverlapRange,
  isSegmentStackedAbove,
  rangesOverlap,
} from "@/components/timeline/subtitleTrackLayout";
import {
  analyzeKeystrokeSegments,
  buildRawEventIndex,
  getVisualActivityRanges,
  type TypedTimeRange,
} from "@/components/timeline/keystrokeTrackRanges";

// Deterministic pseudo-random generator so failures are reproducible.
function random(seed: number) {
  let state = seed;
  return () => {
    state = (state * 1_103_515_245 + 12_345) % 2_147_483_648;
    return state / 2_147_483_648;
  };
}

// The per-segment scan the lanes used before the sweep, kept as the oracle.
function bruteForceStack(segments: Array<{ id: string; startTime: number; endTime: number }>, ranks: number[]) {
  return segments.map((segment, index) => {
    const hasCoveredSegmentUnderneath = segments.some((other, otherIndex) =>
      other.id !== segment.id && rangesOverlap(segment, other)
      && !isSegmentStackedAbove(index, ranks[index], otherIndex, ranks[otherIndex]));
    const overlapRanges = segments
      .map((other, otherIndex) => {
        if (other.id === segment.id) return null;
        if (!isSegmentStackedAbove(index, ranks[index], otherIndex, ranks[otherIndex])) return null;
        return getOverlapRange(segment, other);
      })
      .filter((range): range is NonNullable<typeof range> => Boolean(range));
    return { hasCoveredSegmentUnderneath, contentMaskStyle: buildContentMaskStyle(overlapRanges) };
  });
}

describe("timeline overlap sweeps", () => {
  it("matches the per-segment stacking scan for random overlapping lanes", () => {
    for (let seed = 1; seed <= 25; seed += 1) {
      const next = random(seed);
      const segments = Array.from({ length: 60 }, (_, index) => {
        const startTime = next() * 100;
        return { id: `s${index}`, startTime, endTime: startTime + next() * 8 };
      });
      const ranks = segments.map(() => 1 + Math.floor(next() * 3));
      expect(computeSegmentStackOverlaps(segments, (index) => ranks[index]))
        .toEqual(bruteForceStack(segments, ranks));
    }
  });

  it("compares only nearby segments on a long sequential lane", () => {
    const segments = Array.from({ length: 2_000 }, (_, index) => ({
      id: `s${index}`, startTime: index, endTime: index + 1.5,
    }));
    let rankCalls = 0;
    const result = computeSegmentStackOverlaps(segments, () => {
      rankCalls += 1;
      return 1;
    });
    expect(rankCalls).toBe(segments.length);
    expect(result[0].hasCoveredSegmentUnderneath).toBe(false);
    expect(result[1].hasCoveredSegmentUnderneath).toBe(true);
    expect(result[0].contentMaskStyle).toBeDefined();
  });

  it("matches a brute-force keystroke analysis", () => {
    const next = random(7);
    const raw: TypedTimeRange[] = Array.from({ length: 400 }, () => {
      const startTime = next() * 300;
      return { startTime, endTime: startTime + next() * 3, kind: next() > 0.5 ? "keyboard" : "mouse" };
    }).sort((a, b) => a.startTime - b.startTime);
    const activity = getVisualActivityRanges(raw, 300);
    const segments = Array.from({ length: 80 }, (_, index) => ({
      id: `k${index}`, startTime: index * 3.7, endTime: index * 3.7 + 2,
    }));

    const analysis = analyzeKeystrokeSegments(segments, activity, buildRawEventIndex(raw));
    segments.forEach((segment, index) => {
      const expectedIndicators = activity
        .map((range) => ({
          startTime: Math.max(segment.startTime, range.startTime),
          endTime: Math.min(segment.endTime, range.endTime),
        }))
        .filter((range) => range.endTime - range.startTime > 0.001);
      const overlapping = raw.filter((range) =>
        Math.min(segment.endTime, range.endTime) - Math.max(segment.startTime, range.startTime) > 0.001);
      expect(analysis[index].indicatorRanges).toEqual(expectedIndicators);
      expect(analysis[index].kinds).toEqual({
        hasKeyboard: overlapping.some((range) => range.kind === "keyboard"),
        hasMouse: overlapping.some((range) => range.kind === "mouse"),
      });
    });
  });
});
