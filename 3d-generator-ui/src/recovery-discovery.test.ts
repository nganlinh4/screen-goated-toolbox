import test from "node:test";
import assert from "node:assert/strict";
import { matchingPublishedHistory, RecoveryDiscovery } from "./recovery-discovery.ts";
import type { JobStatus, QueueItem } from "./types.ts";

test("a removed local submission is not rediscovered while other recovery is pending", () => {
  const discovery = new RecoveryDiscovery();
  const finish = discovery.beginSubmission();
  assert.equal(discovery.canDiscover(), false);
  finish("local-job");
  assert.equal(discovery.canDiscover(), true);
  assert.equal(discovery.isUnseen("local-job"), false);
  assert.equal(discovery.isUnseen("recovered-job"), true);
});

test("overlapping submissions keep discovery paused until all identities are known", () => {
  const discovery = new RecoveryDiscovery();
  const first = discovery.beginSubmission();
  const second = discovery.beginSubmission();
  first("first");
  first();
  assert.equal(discovery.canDiscover(), false);
  second("second");
  assert.equal(discovery.canDiscover(), true);
  assert.equal(discovery.isUnseen("first"), false);
  assert.equal(discovery.isUnseen("second"), false);
});

test("a late completed recovery joins matching history without merging a running revision", () => {
  const history = { historyId: "history", state: "done", result: { outputPath: "C:\\Models\\Result.glb" } } as QueueItem;
  const status = { stage: "done", outputPath: "c:\\models\\result.glb" } as JobStatus;
  assert.equal(matchingPublishedHistory(status, [history]), history);
  assert.equal(matchingPublishedHistory({ ...status, stage: "refining" }, [history]), undefined);
  assert.equal(matchingPublishedHistory({ ...status, outputPath: "C:\\Models\\other.glb" }, [history]), undefined);
});
