import assert from "node:assert/strict";
import test from "node:test";
import { previewModelPath, settledModelPath } from "./revision-preview.ts";
import type { QueueItem } from "./types";

function item(overrides: Partial<QueueItem>): QueueItem {
  return {
    id: "item",
    batchId: "batch",
    path: "source.png",
    sourceProvenance: "presentation",
    name: "source.png",
    extension: "PNG",
    generationMode: "quality",
    polycount: 5000,
    autoSegment: false,
    submitted: true,
    state: "queued",
    ...overrides,
  };
}

test("a revision being created keeps showing the parent model it started from", () => {
  const revision = item({
    state: "running",
    retainedModelPath: "C:\\Models\\parent.glb",
    result: { stage: "refining", progressText: "", outputPath: "C:\\Models\\parent.glb" },
  });
  assert.equal(settledModelPath(revision), undefined);
  assert.equal(previewModelPath(revision), "C:\\Models\\parent.glb");
});

test("the parent stays visible even when progress updates drop the output path", () => {
  const revision = item({
    state: "running",
    retainedModelPath: "C:\\Models\\parent.glb",
    result: { stage: "finalizing", progressText: "" },
  });
  assert.equal(previewModelPath(revision), "C:\\Models\\parent.glb");
});

test("a finished revision switches to its own artifact", () => {
  const revision = item({
    state: "done",
    retainedModelPath: "C:\\Models\\parent.glb",
    result: { stage: "done", progressText: "", outputPath: "C:\\Models\\child.glb" },
  });
  assert.equal(previewModelPath(revision), "C:\\Models\\child.glb");
});

test("a first generation with nothing to show yet keeps the placeholder", () => {
  const generation = item({
    state: "running",
    result: { stage: "generating", progressText: "" },
  });
  assert.equal(previewModelPath(generation), undefined);
});

test("segmentation in place keeps the model it is segmenting", () => {
  const generation = item({
    state: "running",
    result: { stage: "segmenting", progressText: "", outputPath: "C:\\Models\\model.glb" },
  });
  assert.equal(settledModelPath(generation), "C:\\Models\\model.glb");
});

test("a failed revision falls back to the last artifact it still names", () => {
  const revision = item({
    state: "failed",
    retainedModelPath: "C:\\Models\\parent.glb",
    result: { stage: "failed", progressText: "", outputPath: "C:\\Models\\parent.glb" },
  });
  assert.equal(previewModelPath(revision), "C:\\Models\\parent.glb");
});
