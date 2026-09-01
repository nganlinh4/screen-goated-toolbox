import assert from "node:assert/strict";
import test from "node:test";

import { generationSettings } from "./generation-mode.ts";

test("Fast remains selectable and owns its limits", () => {
  assert.deepEqual(generationSettings("fast", 20_000, true), {
    mode: "fast",
    polycount: 15_000,
    minimumPolycount: 100,
    maximumPolycount: 15_000,
    autoSegment: false,
    showAutoSegment: false,
  });
});

test("Quality remains the separated default flow", () => {
  assert.deepEqual(generationSettings("quality", 100, true), {
    mode: "quality",
    polycount: 500,
    minimumPolycount: 500,
    maximumPolycount: 20_000,
    autoSegment: true,
    showAutoSegment: true,
  });
});
