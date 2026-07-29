import assert from "node:assert/strict";
import test from "node:test";
import { shouldConstructEditableSurface } from "./svg-display-policy.ts";

test("selecting a completed SVG remains preview-only", () => {
  assert.equal(shouldConstructEditableSurface("preview", "done", "result.svg"), false);
});

test("only an explicit edit intent builds editable geometry", () => {
  assert.equal(shouldConstructEditableSurface("edit", "done", "result.svg"), true);
  assert.equal(shouldConstructEditableSurface("edit", "generating", "result.svg"), false);
  assert.equal(shouldConstructEditableSurface("edit", "done", undefined), false);
});
