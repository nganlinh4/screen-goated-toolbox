import assert from "node:assert/strict";
import test from "node:test";
import { retainPublishedDownload, savedResultFiles } from "./result-files.ts";

test("a quad result names its editable download and preview", () => {
  assert.deepEqual(
    savedResultFiles({
      stage: "done",
      progressText: "",
      downloadName: "quad-result.fbx",
      outputName: "quad-result.glb",
    }),
    ["quad-result.fbx", "quad-result.glb"],
  );
});

test("a native glb result is named once", () => {
  assert.deepEqual(
    savedResultFiles({
      stage: "done",
      progressText: "",
      outputName: "triangle-result.glb",
    }),
    ["triangle-result.glb"],
  );
});

test("background separation keeps the published quad download visible", () => {
  const previous = {
    stage: "done",
    progressText: "Model ready",
    outputPath: "base.glb",
    outputName: "base.glb",
    downloadPath: "base.fbx",
    downloadName: "base.fbx",
  };
  assert.deepEqual(
    retainPublishedDownload(previous, {
      stage: "segmenting",
      progressText: "Separating parts",
      outputPath: "base.glb",
      outputName: "base.glb",
    }),
    {
      stage: "segmenting",
      progressText: "Separating parts",
      outputPath: "base.glb",
      outputName: "base.glb",
      downloadPath: "base.fbx",
      downloadName: "base.fbx",
    },
  );
  assert.equal(
    retainPublishedDownload(previous, {
      stage: "done",
      progressText: "Parts ready",
      outputPath: "parts.glb",
    }).downloadPath,
    undefined,
  );
});
