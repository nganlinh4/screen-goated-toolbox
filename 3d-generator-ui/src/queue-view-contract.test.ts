import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const source = readFileSync(new URL("./queue-view.ts", import.meta.url), "utf8");

test("project reconciliation preserves keyed rows and hydrates only visible thumbnails", () => {
  assert.doesNotMatch(source, /queueList\.replaceChildren/);
  assert.match(source, /new IntersectionObserver/);
  assert.match(source, /this\.rows\.get\(item\.id\)/);
  assert.match(source, /fragment\.append\(row\)/);
  assert.match(source, /onThumbnailNeeded/);
  assert.match(source, /querySelectorAll<HTMLElement>\("\.queue-item\[data-item-id\]"\)/);
});

test("thumbnail completion changes image content without replacing the interactive row", () => {
  assert.match(source, /thumbnail\.replaceChildren\(image\)/);
  assert.doesNotMatch(source, /rowSignature[^;]+thumbnailUrl/s);
});
