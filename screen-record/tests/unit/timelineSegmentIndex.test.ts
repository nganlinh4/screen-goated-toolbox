import { describe, expect, it } from "vitest";
import {
  TimelineSegmentIndex,
  buildTimelineRenderWindow,
  type TimelineRangeSegment,
} from "@/components/timeline/timelineSegmentIndex";

function seg(id: string, startTime: number, endTime: number): TimelineRangeSegment {
  return { id, startTime, endTime };
}

describe("TimelineSegmentIndex", () => {
  it("queries visible ranges without scanning unrelated tails", () => {
    const index = new TimelineSegmentIndex([
      seg("late", 50, 60),
      seg("early", 0, 2),
      seg("middle", 10, 12),
      seg("overlap", 8, 15),
    ]);

    expect(index.query({ startTime: 9, endTime: 11 }).map((entry) => entry.id)).toEqual([
      "overlap",
      "middle",
    ]);
  });

  it("hit tests the latest segment whose interval contains the time", () => {
    const index = new TimelineSegmentIndex([
      seg("a", 0, 5),
      seg("b", 3, 7),
      seg("c", 8, 9),
    ]);

    expect(index.hitTest(4)?.id).toBe("b");
    expect(index.hitTest(7.5)).toBeNull();
    expect(index.hitTest(8.5)?.id).toBe("c");
  });

  it("hit tests long earlier intervals even when a later short segment misses", () => {
    const index = new TimelineSegmentIndex([
      seg("long", 0, 100),
      seg("short", 20, 21),
    ]);

    expect(index.hitTest(50)?.id).toBe("long");
  });

  it("queries long earlier intervals that overlap the visible range", () => {
    const index = new TimelineSegmentIndex([
      seg("long", 0, 100),
      seg("short", 20, 21),
      seg("later", 120, 121),
    ]);

    expect(index.query({ startTime: 75, endTime: 76 }).map((entry) => entry.id)).toEqual(["long"]);
  });

  it("keeps dense DOM windows bounded but always includes selected and active segments", () => {
    const segments = Array.from({ length: 1_000 }, (_, index) =>
      seg(`s${index}`, index * 0.1, index * 0.1 + 0.04),
    );
    const windowed = buildTimelineRenderWindow({
      segments,
      duration: 100,
      canvasWidthPx: 1000,
      visibleRange: { startTime: 10, endTime: 20 },
      denseMode: true,
      selectedIds: new Set(["s900"]),
      activeIds: new Set(["s1"]),
      minInteractivePx: 7,
    });

    expect(windowed.canvasSegments.length).toBeGreaterThan(0);
    expect(windowed.canvasSegments.every((entry) => entry.endTime >= 10 && entry.startTime <= 20)).toBe(true);
    expect(windowed.domSegments.map((entry) => entry.id)).toEqual(["s1", "s900"]);
  });

  it("does not render every dense segment as DOM while the viewport range is still unknown", () => {
    const segments = Array.from({ length: 1_000 }, (_, index) =>
      seg(`s${index}`, index * 0.5, index * 0.5 + 0.45),
    );
    const windowed = buildTimelineRenderWindow({
      segments,
      duration: 500,
      canvasWidthPx: 12_000,
      visibleRange: null,
      denseMode: true,
      selectedIds: new Set(["s900"]),
      activeIds: new Set(["s1"]),
      minInteractivePx: 7,
    });

    expect(windowed.canvasSegments.length).toBe(1_000);
    expect(windowed.domSegments.map((entry) => entry.id)).toEqual(["s1", "s900"]);
  });

  it("falls back to indexed hit testing when a dense visible range still has too many DOM candidates", () => {
    const segments = Array.from({ length: 1_000 }, (_, index) =>
      seg(`s${index}`, index * 0.1, index * 0.1 + 0.09),
    );
    const windowed = buildTimelineRenderWindow({
      segments,
      duration: 100,
      canvasWidthPx: 12_000,
      visibleRange: { startTime: 0, endTime: 100 },
      denseMode: true,
      selectedIds: new Set(["s900"]),
      activeIds: new Set(["s1"]),
      minInteractivePx: 7,
      maxInteractiveSegments: 180,
    });

    expect(windowed.canvasSegments.length).toBe(1_000);
    expect(windowed.domSegments.map((entry) => entry.id)).toEqual(["s1", "s900"]);
    expect(windowed.index.hitTest(50.05)?.id).toBe("s500");
  });
});
