import { describe, expect, it } from "vitest";
import { getTimelineComplexityHint } from "@/lib/exportEstimator";
import type { VideoSegment } from "@/types/video";

describe("export complexity hint", () => {
  it("counts canonical zoom blocks", () => {
    const blocks = Array.from({ length: 9 }, (_, index) => ({ id: `z${index}` }));
    const withBlocks = { trimStart: 0, trimEnd: 10, zoomBlocks: blocks } as unknown as VideoSegment;
    const withoutZoom = { trimStart: 0, trimEnd: 10, zoomBlocks: [] } as unknown as VideoSegment;
    expect(getTimelineComplexityHint(withoutZoom)).toBe(0);
    expect(getTimelineComplexityHint(withBlocks)).toBeCloseTo(0.07);
  });
});
