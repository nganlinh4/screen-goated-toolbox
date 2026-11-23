import { describe, expect, it } from "vitest";
import {
  buildPointsFromRanges,
  getRangesFromPoints,
  resolveRanges,
  type SoftAudioPoint,
  type SoftAudioRange,
} from "@/components/timeline/softAudioRanges";

const DURATION = 10;

// One block from 1s to 5s with a custom ducking dip to 0.2 around 3s.
const duckedEnvelope: SoftAudioPoint[] = [
  { time: 0, volume: 0 },
  { time: 1, volume: 0 },
  { time: 1.12, volume: 1 },
  { time: 2.5, volume: 1 },
  { time: 3, volume: 0.2 },
  { time: 3.5, volume: 1 },
  { time: 4.88, volume: 1 },
  { time: 5, volume: 0 },
  { time: DURATION, volume: 0 },
];

function rebuild(points: SoftAudioPoint[], edit: (ranges: SoftAudioRange[]) => SoftAudioRange[]) {
  const ranges = getRangesFromPoints(points, DURATION);
  const sourced = ranges.map((range) => ({ ...range, sourceId: range.id }));
  return buildPointsFromRanges(DURATION, resolveRanges(edit(sourced), DURATION), points, ranges);
}

function volumeAt(points: SoftAudioPoint[], time: number) {
  return points.find((point) => Math.abs(point.time - time) < 1e-9)?.volume;
}

describe("compact audio block edits", () => {
  it("keeps interior automation when a block edge is trimmed", () => {
    const next = rebuild(duckedEnvelope, ([range]) => [{ ...range, endTime: 4.5 }]);
    expect(volumeAt(next, 3)).toBeCloseTo(0.2);
    expect(volumeAt(next, 2.5)).toBe(1);
    expect(volumeAt(next, 4.5)).toBe(0);
  });

  it("moves interior automation with a body drag", () => {
    const next = rebuild(duckedEnvelope, ([range]) => [
      { ...range, startTime: 3, endTime: 7, offset: 2 },
    ]);
    expect(volumeAt(next, 5)).toBeCloseTo(0.2);
    expect(volumeAt(next, 3)).toBe(0);
  });

  it("keeps the dip on the half that contains it after a split", () => {
    const next = rebuild(duckedEnvelope, ([range]) => [
      { ...range, endTime: 3.98 },
      { ...range, id: `${range.id}-split`, startTime: 4.02 },
    ]);
    expect(volumeAt(next, 3)).toBeCloseTo(0.2);
    expect(getRangesFromPoints(next, DURATION)).toHaveLength(2);
  });

  it("adds new blocks flat at their volume", () => {
    const next = rebuild(duckedEnvelope, (ranges) => [
      ...ranges,
      { id: "added", startTime: 7, endTime: 9, volume: 0.6 },
    ]);
    expect(volumeAt(next, 3)).toBeCloseTo(0.2);
    expect(volumeAt(next, 7.12)).toBeCloseTo(0.6);
    expect(volumeAt(next, 8.88)).toBeCloseTo(0.6);
  });
});
