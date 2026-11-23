import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { cancellationOutcome } from "./cancellation-outcome.ts";
import type { JobStatus } from "./types.ts";

const busy = new Set(["queued", "preparing", "generating", "segmenting", "refining", "finalizing"]);
const status = (stage: JobStatus["stage"], extra: Partial<JobStatus> = {}): JobStatus => ({
  jobId: "job-1", stage, progressText: stage, ...extra,
});

test("a confirmed cancellation settles the item as cancelled", () => {
  assert.deepEqual(
    cancellationOutcome("job-1", status("cancelled"), busy),
    { kind: "settled", status: status("cancelled") },
  );
});

test("a job that finished before the cancel arrived keeps its published model", () => {
  const done = status("done", { outputPath: "C:/out/model.glb", outputName: "model.glb" });
  const outcome = cancellationOutcome("job-1", done, busy);
  assert.equal(outcome.kind, "settled");
  assert.equal(outcome.kind === "settled" && outcome.status.stage, "done");
  assert.equal(outcome.kind === "settled" && outcome.status.outputPath, "C:/out/model.glb");
});

test("a cancel the host could not persist leaves the job tracked as running", () => {
  assert.deepEqual(
    cancellationOutcome("job-1", status("generating"), busy),
    { kind: "running", status: status("generating") },
  );
});

test("a reply about another job, or no reply, does not settle this job", () => {
  // The host answers with its latest status when it no longer knows the id.
  assert.deepEqual(cancellationOutcome("job-1", status("cancelled", { jobId: "job-2" }), busy), {
    kind: "unconfirmed",
  });
  assert.deepEqual(cancellationOutcome("job-1", undefined, busy), { kind: "unconfirmed" });
});

test("every cancel reply in the job runner is judged before it settles an item", () => {
  // A cancel that lands while start_job is in flight must not settle the item
  // from a still-running or foreign status any more than the cancel button may.
  const source = readFileSync(new URL("./job-runner.ts", import.meta.url), "utf8");
  const requests = source.match(/"cancel_job"/g) ?? [];
  const judged = [...source.matchAll(/const (\w+) = await invoke<JobStatus>\("cancel_job"/g)]
    .filter(([, reply]) => new RegExp(String.raw`cancellationOutcome\([^)]*\b${reply}\b`).test(source));
  assert.ok(requests.length > 0);
  assert.equal(judged.length, requests.length);
});
