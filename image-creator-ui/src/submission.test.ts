import assert from "node:assert/strict";
import test from "node:test";
import {
  canSubmitImageSelection,
  ExplicitSubmissionTracker,
  selectionAfterSubmission,
  startImageArguments,
  SurfaceSourceRegistry,
} from "./submission.ts";

test("zero references omit image input while duplicates preserve order", () => {
  assert.deepEqual(startImageArguments([], "out", "create", "none"), {
    outputDir: "out",
    prompt: "create",
  });
  assert.deepEqual(
    startImageArguments(
      ["a.png", "b.png", "a.png"],
      "out",
      "edit",
      "surface-import",
    ).imagePaths,
    ["a.png", "b.png", "a.png"],
  );
});

test("history presentations are display-only while text-only history remains rerunnable", () => {
  assert.equal(canSubmitImageSelection([], "none"), true);
  assert.equal(canSubmitImageSelection(["fresh.png"], "surface-import"), true);
  assert.equal(canSubmitImageSelection(["preview.png"], "presentation"), false);
  assert.throws(
    () => startImageArguments(["preview.png"], "out", "edit", "presentation"),
    /current surface/,
  );
});

test("same-surface source provenance is copied and bounded", () => {
  const sources = new SurfaceSourceRegistry(2);
  const original = ["one.png"];
  sources.remember("one", original);
  original[0] = "changed.png";
  sources.remember("two", []);
  sources.remember("three", ["three.png"]);

  assert.equal(sources.references("one"), undefined);
  assert.deepEqual(sources.references("two"), []);
  const returned = sources.references("three")!;
  returned[0] = "mutated.png";
  assert.deepEqual(sources.references("three"), ["three.png"]);
});

test("three rapid presses on one session are all distinct and active", () => {
  let sequence = 0;
  const submissions = new ExplicitSubmissionTracker(() => `nonce-${++sequence}`);
  const tickets = [
    submissions.begin("session-a"),
    submissions.begin("session-a"),
    submissions.begin("session-a"),
  ];

  assert.equal(new Set(tickets.map((ticket) => ticket.id)).size, 3);
  assert.deepEqual(submissions.activeIds(), tickets.map((ticket) => ticket.id).sort());
  assert.equal(submissions.isLatest(tickets[0]), false);
  assert.equal(submissions.isLatest(tickets[1]), false);
  assert.equal(submissions.isLatest(tickets[2]), true);
});

test("out-of-order responses select only the latest press and never another session", () => {
  let sequence = 0;
  const submissions = new ExplicitSubmissionTracker(() => `nonce-${++sequence}`);
  const first = submissions.begin("session-a");
  const latest = submissions.begin("session-a");
  const other = submissions.begin("session-b");

  assert.equal(
    selectionAfterSubmission("session-a", first, "job-first", submissions.isLatest(first)),
    "session-a",
  );
  assert.equal(
    selectionAfterSubmission("session-a", latest, "job-latest", submissions.isLatest(latest)),
    "job-latest",
  );
  assert.equal(
    selectionAfterSubmission("session-b", latest, "job-latest", submissions.isLatest(latest)),
    "session-b",
  );
  assert.equal(
    selectionAfterSubmission("session-b", other, "job-other", submissions.isLatest(other)),
    "job-other",
  );
});

test("finishing tickets releases bookkeeping without promoting an older press", () => {
  let sequence = 0;
  const submissions = new ExplicitSubmissionTracker(() => `nonce-${++sequence}`);
  const older = submissions.begin("session-a");
  const latest = submissions.begin("session-a");
  submissions.finish(latest);
  assert.equal(submissions.isLatest(older), false);
  submissions.finish(older);
  assert.deepEqual(submissions.activeIds(), []);
});
