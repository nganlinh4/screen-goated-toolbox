import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it, vi } from "vitest";

vi.mock("@/hooks/useSettings", () => ({ useSettings: () => ({ t: {}, lang: "en" }) }));

const srcRoot = path.resolve(import.meta.dirname, "../../src");
const read = (relative: string) => readFileSync(path.join(srcRoot, relative), "utf8");

describe("preview module surface", () => {
  it("keeps PlaybackControls in its own module, not in the VideoPreview grab bag", async () => {
    const preview = await import("@/components/VideoPreview");
    const controls = await import("@/components/PlaybackControls");
    expect(Object.keys(preview).sort()).toEqual(["Placeholder", "SeekIndicator"]);
    expect(typeof controls.PlaybackControls).toBe("function");
  });

  it("has no export progress channel that nothing feeds", () => {
    // The exporter never invoked onProgress; the overlay reads progress events itself.
    expect(read("types/videoExportTypes.ts")).not.toMatch(/\bonProgress\b/);
    expect(read("hooks/useExport.ts")).not.toMatch(/exportProgress/i);
    expect(read("components/dialogs/index.tsx")).not.toMatch(/exportProgress/);
  });
});
