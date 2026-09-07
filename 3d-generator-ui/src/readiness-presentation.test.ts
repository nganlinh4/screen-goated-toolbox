import assert from "node:assert/strict";
import test from "node:test";
import { readinessIsError, readinessLabel } from "./readiness-presentation.ts";

test("active work takes precedence over reserve availability", () => {
  assert.equal(readinessLabel(true, true, "unavailable"), "working");
  assert.equal(readinessIsError(true, true), false);
});

test("idle readiness preserves preparing, ready, and unavailable states", () => {
  assert.equal(readinessLabel(false, true, "unavailable"), "unavailable");
  assert.equal(readinessLabel(false, false, "ready"), "ready");
  assert.equal(readinessLabel(false, false, "preparing"), "preparing");
});
