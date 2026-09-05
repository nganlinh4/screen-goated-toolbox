import assert from "node:assert/strict";
import test from "node:test";
import {
  AUTOMATIC_SEGMENTATION_START,
  DEFAULT_PROGRESS_RANGE,
  GENERATION_WITH_SEGMENTATION_RANGE,
  automaticSegmentationRange,
  nextDisplayedProgress,
  pendingRevisionStatus,
} from "./progress-policy.ts";

test("a new revision clears its parent's operation identity and completed timing", () => {
  const parent = {
    jobId: "parent", stage: "done" as const, progressText: "Done",
    progressRatio: 1, elapsedMs: 200_000, estimatedTotalMs: 240_000,
    timingSampleCount: 24, phase: "complete", outputPath: "parent.glb",
    canRefine: true, supportedActions: ["rig"],
  };
  const child = pendingRevisionStatus(parent, "Creating");
  assert.equal(child.jobId, undefined);
  assert.equal(child.parentRevisionId, "parent");
  assert.equal(child.elapsedMs, 0);
  assert.equal(child.estimatedTotalMs, undefined);
  assert.equal(child.timingSampleCount, 0);
  assert.equal(child.phase, undefined);
  assert.equal(child.outputPath, "parent.glb");
  assert.equal(child.canRefine, false);
  assert.deepEqual(child.supportedActions, []);
  const start = nextDisplayedProgress(0, 0, 120_000, child.progressRatio!, DEFAULT_PROGRESS_RANGE);
  assert.equal(start, 0);
  assert.ok(nextDisplayedProgress(start, 1000, 120_000, 0, DEFAULT_PROGRESS_RANGE) > start);
  assert.equal(parent.progressRatio, 1);
  assert.equal(parent.jobId, "parent");
});

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
