import assert from "node:assert/strict";
import test from "node:test";
import {
  advanceMissingStatusPoll,
  claimNextSubmission,
  dispatchAllSubmissions,
} from "./durable-dispatch.ts";
import type { QueueItem } from "./types.ts";

function item(id: string, state: QueueItem["state"]): QueueItem {
  return {
    id,
    batchId: id,
    path: `${id}.png`,
    sourceProvenance: "surface-import",
    name: `${id}.png`,
    extension: "PNG",
    generationMode: "quality",
    polycount: 5_000,
    autoSegment: false,
    submitted: true,
    state,
  };
}

test("a press is dispatched once even while two jobs are running", () => {
  const jobs = [item("running-a", "running"), item("running-b", "running"), item("pressed", "queued")];
  let starts = 0;
  const claimed = claimNextSubmission(jobs);
  assert.equal(claimed?.id, "pressed");
  starts += 1;
  claimed!.result = { jobId: "durable-id", stage: "queued", progressText: "Queued" };

  assert.equal(claimNextSubmission(jobs), undefined);
  assert.equal(starts, 1);
  assert.equal(claimed!.result?.stage, "queued");
});

test("a presentation-only recovered item is never dispatched", () => {
  const recovered = item("recovered", "queued");
  recovered.sourceProvenance = "presentation";
  assert.equal(claimNextSubmission([recovered]), undefined);
  assert.equal(recovered.state, "queued");
});

test("later admissions do not wait for an earlier job to finish", async () => {
  const jobs = [item("first", "queued"), item("second", "queued")];
  const starts: string[] = [];
  await dispatchAllSubmissions(jobs, async (claimed) => {
    starts.push(claimed.id);
    claimed.result = {
      jobId: `job-${claimed.id}`,
      stage: "queued",
      progressText: "Queued",
    };
  });
  assert.deepEqual(starts, ["first", "second"]);
  assert.deepEqual(jobs.map((job) => job.result?.stage), ["queued", "queued"]);
});

test("a missing durable status resolves after a bounded successful-poll window", () => {
  assert.deepEqual(advanceMissingStatusPoll(73, 75), {
    count: 74,
    timedOut: false,
  });
  assert.deepEqual(advanceMissingStatusPoll(74, 75), {
    count: 75,
    timedOut: true,
  });
});
