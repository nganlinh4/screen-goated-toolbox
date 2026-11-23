import { describe, expect, it } from "vitest";
import { buildSubtitleSrt, parseSubtitleSrt } from "@/lib/subtitleSrt";
import type { SubtitleSegment } from "@/types/video";

function cue(id: string, startTime: number, endTime: number, text: string): SubtitleSegment {
  return { id, startTime, endTime, text } as SubtitleSegment;
}

describe("SRT export round trip", () => {
  it("carries a sub-millisecond remainder into the next second instead of writing ,1000", () => {
    // Float edits routinely land just below a whole second.
    const srt = buildSubtitleSrt([cue("a", 0.1 + 0.2, 2.9996, "hello")]);
    expect(srt).toContain("00:00:00,300 --> 00:00:03,000");
    expect(srt).not.toMatch(/,\d{4}/);
    const [parsed] = parseSubtitleSrt(srt, 10);
    expect(parsed?.endTime).toBeCloseTo(3, 6);
  });

  it("rolls over minutes and hours too", () => {
    expect(buildSubtitleSrt([cue("a", 3599.9999, 3600.5, "x")])).toContain("01:00:00,000 --> 01:00:00,500");
  });

  it("keeps a cue whose text has blank lines as one cue", () => {
    const srt = buildSubtitleSrt([
      cue("a", 1, 2, "first paragraph\n\nsecond paragraph"),
      cue("b", 3, 4, "next"),
    ]);
    const parsed = parseSubtitleSrt(srt, 10);
    expect(parsed.map((item) => item.text)).toEqual(["first paragraph\nsecond paragraph", "next"]);
  });
});
