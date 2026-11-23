import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";

import { generationSettings } from "./generation-mode.ts";
import { frozenGenerationSettings } from "./recovery-settings.ts";
import { canSubmitItem } from "./submission-policy.ts";

test("new requests and saved modes consume the canonical admission fixture", () => {
  const fixture = JSON.parse(readFileSync(new URL("../../parity-fixtures/image-to-3d/generation-mode-contract.json", import.meta.url), "utf8"));
  for (const row of fixture.newRequests) {
    const candidate = { sourceProvenance: "surface-import" as const, generationMode: row.mode };
    assert.equal(canSubmitItem(candidate), fixture.currentModes.includes(row.mode));
    if (row.normalizedPolycount !== null) {
      assert.equal(generationSettings(row.mode, row.polycount, row.autoSegment).polycount, row.normalizedPolycount);
    }
  }
  for (const row of fixture.frozenRequests) {
    const saved = frozenGenerationSettings({ ...row, generationMode: row.mode, outputDir: "library" });
    assert.equal(saved !== undefined, row.valid);
    if (saved) {
      assert.equal(saved.generationMode, row.mode);
      assert.equal(saved.polycount, row.polycount);
      assert.equal(saved.autoSegment, row.autoSegment);
    }
  }
});

test("Quality remains the separated default flow", () => {
  assert.deepEqual(generationSettings("quality", 100, true), {
    mode: "quality",
    polycount: 500,
    minimumPolycount: 500,
    maximumPolycount: 20_000,
    autoSegment: true,
    showAutoSegment: true,
    topology: "triangle",
  });
});

test("initial topology is independent unless automatic separation needs triangles", () => {
  assert.equal(generationSettings("quality", 5000, false, "triangle").topology, "triangle");
  assert.equal(generationSettings("quality", 5000, false, "quad").topology, "quad");
  assert.equal(generationSettings("quality", 5000, true, "quad").topology, "triangle");
});
