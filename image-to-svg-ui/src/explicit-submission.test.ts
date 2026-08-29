import assert from "node:assert/strict";
import test from "node:test";

import {
  canActivatePrimaryAction,
  canSubmitItem,
  freshSubmissionSession,
  needsFreshSubmissionSession,
} from "./explicit-submission.ts";
import type { Item } from "./types.ts";

test("only an untouched draft is consumed by its first explicit press", () => {
  assert.equal(needsFreshSubmissionSession({ stage: "draft" } as Item), false);
  for (const stage of [
    "queued",
    "preparing",
    "generating",
    "finalizing",
    "done",
    "failed",
    "cancelled",
  ] as const) {
    assert.equal(needsFreshSubmissionSession({ stage } as Item), true);
  }
});

test("only a source imported by this surface can create another SVG", () => {
  assert.equal(canSubmitItem({ sourceProvenance: "surface-import" }), true);
  assert.equal(canSubmitItem({ sourceProvenance: "presentation" }), false);
  assert.equal(canSubmitItem({ sourceProvenance: "none" }), false);
});

test("an active SVG job cannot light or trigger the primary action", () => {
  for (const stage of ["queued", "preparing", "generating", "finalizing"] as const) {
    assert.equal(
      canActivatePrimaryAction({ sourceProvenance: "surface-import", stage }),
      false,
    );
  }
  for (const stage of ["done", "failed", "cancelled"] as const) {
    assert.equal(
      canActivatePrimaryAction({ sourceProvenance: "surface-import", stage }),
      true,
    );
  }
});

test("rapid presses clone only the selected session with frozen settings", () => {
  const source: Item = {
    id: "selected",
    batchId: "import",
    path: "selected.png",
    sourceProvenance: "surface-import",
    name: "selected.png",
    model: "detail",
    backgroundMode: "transparent",
    outputDir: "chosen-output",
    stage: "generating",
    submitted: true,
    jobId: "existing-job",
    outputPath: "existing.svg",
    svgText: "<svg/>",
  };
  const first = freshSubmissionSession(source, "press-one");
  const second = freshSubmissionSession(source, "press-two");

  assert.notEqual(first.id, second.id);
  assert.ok(first.createdAtMs);
  assert.ok(second.createdAtMs);
  const { createdAtMs: _firstCreated, ...firstStable } = first;
  const { createdAtMs: _secondCreated, ...secondStable } = second;
  assert.deepEqual(firstStable, {
    id: "press-one",
    batchId: "press-one",
    path: "selected.png",
    sourceProvenance: "surface-import",
    name: "selected.png",
    model: "detail",
    backgroundMode: "transparent",
    outputDir: "chosen-output",
    stage: "queued",
  });
  assert.deepEqual(secondStable, { ...firstStable, id: "press-two", batchId: "press-two" });
});
