import { describe, expect, it, vi } from "vitest";
import { parseSubtitleFile } from "@/lib/subtitleSrt";

vi.stubGlobal("crypto", {
  randomUUID: vi.fn(() => "test-subtitle-id"),
});

function parseVtt(lines: string[]) {
  return parseSubtitleFile({ fileName: "captions.vtt", content: lines.join("\n") }, 60);
}

describe("VTT import", () => {
  it("keeps the first cue after a one-line header block", () => {
    const parsed = parseVtt([
      "WEBVTT",
      "X-TIMESTAMP-MAP=MPEGTS:900000,LOCAL:00:00:00.000",
      "",
      "00:00:01.000 --> 00:00:02.000",
      "First",
      "",
      "00:00:03.000 --> 00:00:04.000",
      "Second",
    ]);

    expect(parsed.map((cue) => cue.text)).toEqual(["First", "Second"]);
  });

  it("does not let a stray single line swallow the next cue", () => {
    const parsed = parseVtt([
      "WEBVTT",
      "",
      "orphan-identifier",
      "",
      "00:00:01.000 --> 00:00:02.000",
      "Kept",
    ]);

    expect(parsed.map((cue) => cue.text)).toEqual(["Kept"]);
  });

  it("still reads a cue identifier on the line before the timing", () => {
    const parsed = parseVtt([
      "WEBVTT - Title",
      "Kind: captions",
      "Language: en",
      "",
      "intro",
      "00:00:01.000 --> 00:00:02.000",
      "Hello",
    ]);

    expect(parsed).toHaveLength(1);
    expect(parsed[0]).toMatchObject({ startTime: 1, endTime: 2, text: "Hello" });
  });

  it("decodes each character reference exactly once", () => {
    const parsed = parseVtt([
      "WEBVTT",
      "",
      "00:00:01.000 --> 00:00:02.000",
      "Use &amp;lt;b&amp;gt; for &lt;bold&gt; &amp; more",
    ]);

    expect(parsed[0]?.text).toBe("Use &lt;b&gt; for <bold> & more");
  });
});
