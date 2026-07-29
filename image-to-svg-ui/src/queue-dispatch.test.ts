import assert from "node:assert/strict";
import test from "node:test";
import {
  advanceMissingStatusPoll,
  claimNextQueued,
} from "./queue-dispatch.ts";
import type { Item } from "./types.ts";

test("a queued start response cannot dispatch the same item twice", async () => {
  const item: Item = {
    id: "one",
    batchId: "one",
    path: "one.png",
    sourceProvenance: "surface-import",
    name: "one.png",
    model: "simple",
    outputDir: "output",
    stage: "queued",
  };
  let calls = 0;
  for (;;) {
    const claimed = claimNextQueued([item]);
    if (!claimed) break;
    calls += 1;
    claimed.jobId = "queued-job";
    claimed.stage = "queued";
  }
  assert.equal(calls, 1);
  assert.equal(item.submitted, true);
});

test("a presentation-only history thumbnail is never dispatched", () => {
  const item: Item = {
    id: "history",
    batchId: "history",
    path: "presentation.png",
    sourceProvenance: "presentation",
    name: "presentation.png",
    model: "simple",
    outputDir: "output",
    stage: "queued",
  };
  assert.equal(claimNextQueued([item]), undefined);
  assert.equal(item.submitted, undefined);
});

test("a missing durable status resolves after a bounded successful-poll window", () => {
  assert.deepEqual(advanceMissingStatusPoll(83, 85), {
    count: 84,
    timedOut: false,
  });
  assert.deepEqual(advanceMissingStatusPoll(84, 85), {
    count: 85,
    timedOut: true,
  });
});
