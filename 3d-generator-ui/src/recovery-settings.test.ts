import assert from "node:assert/strict";
import test from "node:test";
import { frozenGenerationSettings } from "./recovery-settings.ts";

test("recovery restores every frozen setting without deriving it from result state", () => {
  assert.deepEqual(
    frozenGenerationSettings({
      generationMode: "quality",
      polycount: 12_300,
      autoSegment: false,
      instruction: "Keep the silhouette",
      outputDir: "C:\\Creation Library",
    }),
    {
      generationMode: "quality",
      polycount: 12_300,
      autoSegment: false,
      instruction: "Keep the silhouette",
      outputDir: "C:\\Creation Library",
    },
  );
});

test("recovery fails closed when frozen settings are absent or noncanonical", () => {
  assert.equal(frozenGenerationSettings({
    generationMode: "quality",
    autoSegment: false,
    outputDir: "C:\\Creation Library",
  }), undefined);
  assert.equal(frozenGenerationSettings({
    generationMode: "fast",
    polycount: 20_000,
    autoSegment: true,
    outputDir: "C:\\Creation Library",
  }), undefined);
});
