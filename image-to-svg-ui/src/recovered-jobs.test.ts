import assert from "node:assert/strict";
import test from "node:test";
import { adoptRecoveredJobs } from "./recovered-jobs.ts";
import { claimNextQueued } from "./queue-dispatch.ts";
import type { Item, JobStatus } from "./types";

const status = (stage: JobStatus["stage"]): JobStatus => ({
  jobId: "saved", stage, progressText: stage, sourceImagePath: "source.png",
  outputDir: "outputs", model: "simple", backgroundMode: "opaque",
  ...(stage === "failed" ? { error: "Creation timed out." } : {}),
});

test("a recovered request is added once without dispatching it again", () => {
  const items: Item[] = [];
  assert.equal(adoptRecoveredJobs(items, [status("queued")], (path) => path), 1);
  const recovered = items[0];
  assert.equal(recovered.submitted, true);
  assert.equal(claimNextQueued(items), undefined);
  assert.equal(adoptRecoveredJobs(items, [status("queued")], (path) => path), 0);
  assert.equal(items.length, 1);
  assert.equal(items[0], recovered);
});

test("an expired recovered request stays visible as failed without becoming a new draft", () => {
  const items: Item[] = [];
  assert.equal(adoptRecoveredJobs(items, [status("failed")], (path) => path), 1);
  assert.equal(items[0].stage, "failed");
  assert.equal(items[0].error, "Creation timed out.");
  assert.equal(items[0].sourceProvenance, "presentation");
  assert.equal(claimNextQueued(items), undefined);
});
