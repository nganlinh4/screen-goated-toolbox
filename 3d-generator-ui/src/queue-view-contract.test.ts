import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { lineageChains } from "./lineage.ts";

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

test("chain labeling numbers revisions from the root and marks each chain tip", () => {
  const items = [
    { id: "a", historyId: "h1", result: { projectId: "p1", revisionKind: "generation" } },
    { id: "b", historyId: "h2", result: { projectId: "p1", parentRevisionId: "h1", revisionKind: "separate_parts" } },
    { id: "c", historyId: "h3", result: { projectId: "p1", parentRevisionId: "h2", revisionKind: "optimize_triangle" } },
    { id: "d", historyId: "h4", result: { projectId: "p2", revisionKind: "generation" } },
    { id: "e", result: { projectId: "p1", revisionKind: "generation" } },
    { id: "f", historyId: "h5", result: { projectId: "p3", parentRevisionId: "h6", revisionKind: "rig" } },
    { id: "g", historyId: "h6", result: { projectId: "p3", parentRevisionId: "h5", revisionKind: "animate" } },
  ];
  const lineage = lineageChains(items);
  assert.equal(lineage.get("a")?.version, 1);
  assert.equal(lineage.get("a")?.current, false);
  assert.equal(lineage.get("b")?.version, 2);
  assert.equal(lineage.get("b")?.current, false);
  assert.deepEqual(lineage.get("c"), { version: 3, kind: "optimize_triangle", current: true });
  assert.deepEqual(lineage.get("d"), { version: 1, kind: "generation", current: true });
  assert.equal(lineage.has("e"), false);
  // A parent cycle must terminate and leave both nodes labelled.
  assert.equal(lineage.get("f")?.version, 2);
  assert.equal(lineage.get("g")?.version, 2);
  assert.equal(lineage.get("f")?.current, false);
  assert.equal(lineage.get("g")?.current, false);
});
