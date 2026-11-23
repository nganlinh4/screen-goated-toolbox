import { describe, expect, it, vi } from "vitest";
import { parseSubtitleFile, readSubtitleFileText } from "@/lib/subtitleSrt";

vi.stubGlobal("crypto", {
  randomUUID: vi.fn(() => "test-subtitle-id"),
});

const CUE = "1\n00:00:01,000 --> 00:00:02,000\nHello 안\n";

function utf16(text: string, littleEndian: boolean): Uint8Array {
  const bytes = new Uint8Array(2 + text.length * 2);
  const view = new DataView(bytes.buffer);
  view.setUint16(0, 0xfeff, littleEndian);
  for (let index = 0; index < text.length; index += 1) {
    view.setUint16(2 + index * 2, text.charCodeAt(index), littleEndian);
  }
  return bytes;
}

function file(bytes: Uint8Array, name = "captions.srt") {
  return new File([bytes], name);
}

describe("reading a dropped subtitle file", () => {
  it.each([
    ["UTF-16 LE", utf16(CUE, true)],
    ["UTF-16 BE", utf16(CUE, false)],
    ["UTF-8 with BOM", new Uint8Array([0xef, 0xbb, 0xbf, ...new TextEncoder().encode(CUE)])],
    ["UTF-8", new TextEncoder().encode(CUE)],
  ])("decodes %s like the host file reader", async (_, bytes) => {
    const content = await readSubtitleFileText(file(bytes));

    expect(content).toBe(CUE);
    expect(parseSubtitleFile({ content, fileName: "captions.srt" }, 10)).toMatchObject([
      { startTime: 1, endTime: 2, text: "Hello 안" },
    ]);
  });

  it("replaces invalid UTF-8 bytes instead of rejecting the file", async () => {
    const bytes = new Uint8Array([...new TextEncoder().encode("1\n00:00:01,000 --> 00:00:02,000\ncaf"), 0xe9, 0x0a]);
    expect(await readSubtitleFileText(file(bytes))).toContain("caf�");
  });
});
