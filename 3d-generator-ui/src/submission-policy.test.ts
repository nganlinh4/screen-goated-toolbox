import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";

import {
  canSubmitItem,
  freshSubmissionSession,
  needsFreshSubmissionSession,
  submissionInFlight,
} from "./submission-policy.ts";
import type { QueueItem } from "./types.ts";

test("the primary action is hidden only for the selected submitted or running session", () => {
  const fixture = JSON.parse(readFileSync(new URL(
    "../../parity-fixtures/image-to-3d/progress-contract.json", import.meta.url,
  ), "utf8"));
  for (const scenario of fixture.primaryActionHidden) {
    assert.equal(submissionInFlight(scenario), scenario.hidden);
  }
  assert.equal(submissionInFlight(undefined), false);
  assert.equal(submissionInFlight({ state: "queued", submitted: false }), false);
  assert.equal(submissionInFlight({ state: "queued", submitted: true }), true);
  assert.equal(submissionInFlight({ state: "running", submitted: true }), true);
  for (const state of ["done", "failed", "cancelled"] as const) {
    assert.equal(submissionInFlight({ state, submitted: true }), false);
  }
});

test("only a source imported by this surface can create another model", () => {
  assert.equal(canSubmitItem({ sourceProvenance: "surface-import" }), true);
  assert.equal(canSubmitItem({ sourceProvenance: "presentation" }), false);
  assert.equal(canSubmitItem({ sourceProvenance: "none" }), false);
});

test("only the first press consumes a draft and later presses clone frozen settings", () => {
  assert.equal(needsFreshSubmissionSession({ state: "queued", submitted: false }), false);
  assert.equal(needsFreshSubmissionSession({ state: "queued", submitted: true }), true);
  assert.equal(needsFreshSubmissionSession({ state: "running", submitted: true }), true);
  const source: QueueItem = {
    id: "selected",
    batchId: "import",
    path: "source.png",
    sourceProvenance: "surface-import",
    name: "source.png",
    extension: "PNG",
    generationMode: "fast",
    polycount: 8_500,
    autoSegment: false,
    instruction: "keep the shape",
    submitted: true,
    state: "running",
    outputDir: "chosen",
    result: { jobId: "existing", stage: "generating", progressText: "Creating" },
  };

  const first = freshSubmissionSession(source, "press-one", "fallback");
  const second = freshSubmissionSession(source, "press-two", "fallback");
  assert.notEqual(first.id, second.id);
  assert.ok(first.createdAtMs);
  assert.ok(second.createdAtMs);
  assert.deepEqual(
    [first, second].map((item) => [
      item.path,
      item.sourceProvenance,
      item.generationMode,
      item.polycount,
      item.instruction,
      item.outputDir,
      item.submitted,
      item.state,
    ]),
    [
      [
        "source.png",
        "surface-import",
        "fast",
        8_500,
        "keep the shape",
        "chosen",
        true,
        "queued",
      ],
      [
        "source.png",
        "surface-import",
        "fast",
        8_500,
        "keep the shape",
        "chosen",
        true,
        "queued",
      ],
    ],
  );
  assert.equal(first.result, undefined);
  assert.equal(second.result, undefined);
});
