import { describe, expect, it, vi } from "vitest";
import {
  buildSubtitleSrt,
  detectSubtitleFileFormat,
  parseSubtitleFile,
} from "@/lib/subtitleSrt";

vi.stubGlobal("crypto", {
  randomUUID: vi.fn(() => "test-subtitle-id"),
});

describe("subtitle file support", () => {
  it("detects VTT by extension, mime type, and content header", () => {
    expect(detectSubtitleFileFormat({ content: "", fileName: "captions.vtt" })).toBe("vtt");
    expect(detectSubtitleFileFormat({ content: "", mimeType: "text/vtt" })).toBe("vtt");
    expect(detectSubtitleFileFormat({ content: "WEBVTT\n\n00:00.000 --> 00:01.000\nHi" })).toBe("vtt");
  });

  it("parses VTT cues and strips cue markup", () => {
    const parsed = parseSubtitleFile(
      {
        fileName: "captions.vtt",
        content: [
          "WEBVTT",
          "",
          "NOTE ignored",
          "metadata",
          "",
          "cue-1",
          "00:00:01.000 --> 00:00:03.500 align:start",
          "<v Speaker>Hello <b>world</b></v>",
        ].join("\n"),
      },
      10,
    );

    expect(parsed).toHaveLength(1);
    expect(parsed[0]).toMatchObject({
      startTime: 1,
      endTime: 3.5,
      text: "Hello world",
    });
  });

  it("builds SRT in range-local time", () => {
    const srt = buildSubtitleSrt(
      [
        {
          id: "a",
          startTime: 10,
          endTime: 12,
          text: "Range subtitle",
          style: {
            fontSize: 42,
            color: "#fff",
            x: 50,
            y: 80,
          },
        },
      ],
      { startTime: 9, endTime: 13 },
    );

    expect(srt).toContain("00:00:01,000 --> 00:00:03,000");
    expect(srt).toContain("Range subtitle");
  });
});
