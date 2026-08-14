import { describe, expect, it } from "vitest";
import {
  createSpeedSampler,
  getSpeedAtTime,
  prepareSpeedCurve,
} from "@/lib/speedCurve";

describe("speed curve", () => {
  it("sorts once without mutating the editor-owned points", () => {
    const points = [{ time: 2, speed: 3 }, { time: 0, speed: 1 }];
    const prepared = prepareSpeedCurve(points);

    expect(prepared.map((point) => point.time)).toEqual([0, 2]);
    expect(points.map((point) => point.time)).toEqual([2, 0]);
    expect(prepareSpeedCurve(points)).toBe(prepared);
  });

  it("uses cosine interpolation and safe empty-curve defaults", () => {
    const sample = createSpeedSampler([
      { time: 0, speed: 1 },
      { time: 2, speed: 3 },
    ]);

    expect(sample(1)).toBeCloseTo(2, 12);
    expect(sample(2)).toBe(3);
    expect(getSpeedAtTime(4, [])).toBe(1);
  });
});
