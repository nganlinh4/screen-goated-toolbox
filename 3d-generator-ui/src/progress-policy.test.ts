import assert from "node:assert/strict";
import test from "node:test";
import {
  AUTOMATIC_SEGMENTATION_START,
  DEFAULT_PROGRESS_RANGE,
  GENERATION_WITH_SEGMENTATION_RANGE,
  automaticSegmentationRange,
  nextDisplayedProgress,
} from "./progress-policy.ts";

test("progress never regresses when estimates or provider ratios move backward", () => {
  const first = nextDisplayedProgress(0, 90_000, 120_000, 0.7, DEFAULT_PROGRESS_RANGE);
  const next = nextDisplayedProgress(first, 10_000, 300_000, 0.1, DEFAULT_PROGRESS_RANGE);
  assert.equal(next, first);
});

test("automatic separation continues after the generation range", () => {
  const generation = nextDisplayedProgress(
    0,
    240_000,
    240_000,
    1,
    GENERATION_WITH_SEGMENTATION_RANGE,
  );
  assert.equal(generation, AUTOMATIC_SEGMENTATION_START);

  const segmentation = nextDisplayedProgress(
    generation,
    30_000,
    120_000,
    0.3,
    automaticSegmentationRange(generation),
  );
  assert.ok(segmentation >= generation);
});
