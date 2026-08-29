import assert from "node:assert/strict";
import test from "node:test";
import {
  IMAGE_QUEUE_ROWS_DECODE_ARTWORK,
  IMAGE_REFERENCE_LIST_DECODES_ARTWORK,
  selectedImagePreviewPaths,
} from "./preview-policy.ts";

test("queue and reference rows never request artwork thumbnails", () => {
  assert.equal(IMAGE_QUEUE_ROWS_DECODE_ARTWORK, false);
  assert.equal(IMAGE_REFERENCE_LIST_DECODES_ARTWORK, false);
});

test("selected image canvas has a strict two-preview budget", () => {
  assert.deepEqual(selectedImagePreviewPaths(["one.png"]), ["one.png"]);
  assert.deepEqual(
    selectedImagePreviewPaths(["one.png"], "result.png"),
    ["one.png", "result.png"],
  );
  assert.deepEqual(
    selectedImagePreviewPaths(["one.png", "two.png", "three.png"]),
    ["one.png"],
  );
  assert.deepEqual(
    selectedImagePreviewPaths(["one.png", "two.png", "three.png"], "result.png"),
    ["result.png"],
  );
});
