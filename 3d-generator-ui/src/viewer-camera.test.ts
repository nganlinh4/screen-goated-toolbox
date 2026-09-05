import assert from "node:assert/strict";
import test from "node:test";
import { fitDistance, resizeDistanceRatio } from "./viewer-camera.ts";

test("portrait fitting preserves horizontal space without changing landscape fitting", () => {
  const square = fitDistance(1, 34, 1);
  assert.equal(fitDistance(1, 34, 2), square);
  assert.equal(fitDistance(1, 34, 0.5), square * 2);
});

test("viewport distance changes are reversible and do not accumulate on repeated resize", () => {
  assert.equal(resizeDistanceRatio(2, 0.5), 2);
  assert.equal(resizeDistanceRatio(0.5, 2), 0.5);
  assert.equal(resizeDistanceRatio(0.5, 0.5), 1);
  assert.equal(resizeDistanceRatio(2, 3), 1);
});
