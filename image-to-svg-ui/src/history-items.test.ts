import assert from "node:assert/strict";
import test from "node:test";
import { applyHistoryOutput, removeDeletedHistoryItem } from "./history-items.ts";
import type { Item } from "./types";

const item = (id: string, extra: Partial<Item> = {}): Item => ({
  id, batchId: id, path: `${id}.png`, sourceProvenance: "presentation", name: id,
  model: "simple", backgroundMode: "opaque", outputDir: "out", stage: "done", ...extra,
});

test("a deleted history item already dropped by a history refresh leaves other items alone", () => {
  const deleted = item("history_a", { historyId: "a" });
  const running = item("job", { stage: "generating", jobId: "j1", submitted: true });
  // A concurrent history refresh replaced the list without the deleted entry.
  const items = [running];
  const result = removeDeletedHistoryItem(items, deleted, "job");
  assert.deepEqual(items.map((value) => value.id), ["job"]);
  assert.deepEqual(result, { selectedId: "job", selectionChanged: false });
});

test("deleting the selected history item selects its neighbour", () => {
  const first = item("history_a", { historyId: "a" });
  const second = item("history_b", { historyId: "b" });
  const third = item("history_c", { historyId: "c" });
  const items = [first, second, third];
  const result = removeDeletedHistoryItem(items, second, second.id);
  assert.deepEqual(items.map((value) => value.id), ["history_a", "history_c"]);
  assert.deepEqual(result, { selectedId: "history_c", selectionChanged: true });
});

test("deleting the last selected history item clears the selection", () => {
  const only = item("history_a", { historyId: "a" });
  const items = [only];
  assert.deepEqual(removeDeletedHistoryItem(items, only, only.id), {
    selectedId: "", selectionChanged: true,
  });
  assert.equal(items.length, 0);
});

test("a renamed result drops the preview URL issued for its old file", () => {
  const renamed = item("history_a", {
    historyId: "a", outputPath: "C:/out/old.svg", outputName: "old.svg",
    svgPreviewUrl: "https://asset.localhost/token-for-old-path",
  });
  applyHistoryOutput(renamed, { outputPath: "C:/out/new.svg", outputName: "new.svg" });
  assert.equal(renamed.outputPath, "C:/out/new.svg");
  assert.equal(renamed.outputName, "new.svg");
  assert.equal(renamed.svgPreviewUrl, undefined);
});

test("an unchanged result keeps its preview URL", () => {
  const same = item("history_a", {
    historyId: "a", outputPath: "C:/out/a.svg", outputName: "a.svg",
    svgPreviewUrl: "https://asset.localhost/token",
  });
  applyHistoryOutput(same, { outputPath: "C:/out/a.svg", outputName: "a.svg" });
  assert.equal(same.svgPreviewUrl, "https://asset.localhost/token");
});
