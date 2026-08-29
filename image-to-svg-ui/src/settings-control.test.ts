import assert from "node:assert/strict";
import test from "node:test";

import { normalizeBackgroundMode } from "./settings-control.ts";

test("transparent background uses the compatibility-safe opaque default", () => {
  assert.equal(normalizeBackgroundMode(undefined), "opaque");
  assert.equal(normalizeBackgroundMode("opaque"), "opaque");
  assert.equal(normalizeBackgroundMode("auto"), "auto");
  assert.equal(normalizeBackgroundMode("transparent"), "transparent");
  assert.equal(normalizeBackgroundMode("unknown"), "opaque");
});
