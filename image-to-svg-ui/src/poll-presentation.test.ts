import assert from "node:assert/strict";
import test from "node:test";
import {
  svgHistoryPresentationSignature,
  svgStatusChangesPresentation,
} from "./poll-presentation.ts";
import type { Item, JobStatus } from "./types.ts";

const item: Item = {
  id: "item",
  batchId: "item",
  path: "source.png",
  sourceProvenance: "presentation",
  name: "source.png",
  model: "simple",
  outputDir: "output",
  stage: "done",
  outputPath: "result.svg",
  outputName: "result.svg",
};

test("unchanged polling does not invalidate the SVG presentation", () => {
  const status: JobStatus = {
    jobId: "job",
    stage: "done",
    progressText: "",
    sourceImagePath: "source.png",
    outputDir: "output",
    model: "simple",
    outputPath: "result.svg",
    outputName: "result.svg",
  };
  assert.equal(svgStatusChangesPresentation(item, status), false);
  assert.equal(
    svgHistoryPresentationSignature([item], "item"),
    svgHistoryPresentationSignature([{ ...item, progress: 0.7 }], "item"),
  );
  assert.equal(svgStatusChangesPresentation(item, { ...status, stage: "failed" }), true);
});
