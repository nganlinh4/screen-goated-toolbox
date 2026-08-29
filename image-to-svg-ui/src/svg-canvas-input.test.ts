import assert from "node:assert/strict";
import test from "node:test";

import { resolvePointerSelection } from "./svg-canvas-input.ts";

test("a stationary press selects its pointer-down geometry in one interaction", () => {
  assert.deepEqual(resolvePointerSelection(17, false, false), {
    apply: true,
    index: 17,
  });
});

test("a stationary artboard press clears selection", () => {
  assert.deepEqual(resolvePointerSelection(undefined, false, false), {
    apply: true,
    index: undefined,
  });
});

test("pan and cancelled gestures never change selection", () => {
  assert.deepEqual(resolvePointerSelection(3, true, false), { apply: false });
  assert.deepEqual(resolvePointerSelection(3, false, true), { apply: false });
});
