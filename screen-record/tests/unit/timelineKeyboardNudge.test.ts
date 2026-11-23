import { describe, expect, it } from "vitest";
import { nudgeRange } from "@/components/timeline/timelineLaneEdits";

describe("nudgeRange", () => {
  const range = { startTime: 2, endTime: 4 };

  it("moves the whole range and keeps its length", () => {
    expect(nudgeRange(range, "body", 0.5, 10)).toEqual({ startTime: 2.5, endTime: 4.5 });
  });

  it("stops the range at both ends of the video", () => {
    expect(nudgeRange(range, "body", -5, 10)).toEqual({ startTime: 0, endTime: 2 });
    expect(nudgeRange(range, "body", 20, 10)).toEqual({ startTime: 8, endTime: 10 });
  });

  it("moves one edge without crossing the other", () => {
    expect(nudgeRange(range, "start", -1, 10)).toEqual({ startTime: 1, endTime: 4 });
    const shrunk = nudgeRange(range, "start", 5, 10);
    expect(shrunk.endTime).toBe(4);
    expect(shrunk.startTime).toBeLessThan(4);
    const end = nudgeRange(range, "end", 10, 10);
    expect(end).toEqual({ startTime: 2, endTime: 10 });
  });
});
