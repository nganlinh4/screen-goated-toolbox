import assert from "node:assert/strict";
import test from "node:test";
import { shouldStartAutomaticSegmentation } from "./automatic-segmentation.ts";
import type { JobStatus } from "./types.ts";

function status(overrides: Partial<JobStatus> = {}): JobStatus {
  return {
    jobId: "base-job",
    stage: "done",
    progressText: "Model ready",
    canSegment: true,
    isSegmented: false,
    ...overrides,
  };
}

test("automatic segmentation starts only after the base result is published", () => {
  assert.equal(shouldStartAutomaticSegmentation(true, status()), true);
  assert.equal(
    shouldStartAutomaticSegmentation(true, status({ stage: "finalizing" })),
    false,
  );
  assert.equal(shouldStartAutomaticSegmentation(false, status()), false);
});

test("completed or unavailable continuations are never started again", () => {
  assert.equal(
    shouldStartAutomaticSegmentation(true, status({ isSegmented: true })),
    false,
  );
  assert.equal(
    shouldStartAutomaticSegmentation(true, status({ canSegment: false })),
    false,
  );
  assert.equal(
    shouldStartAutomaticSegmentation(true, status({ jobId: null })),
    false,
  );
});
