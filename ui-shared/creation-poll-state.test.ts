import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { setImmediate as settle } from "node:timers/promises";
import { CreationPollState } from "./creation-poll-state.ts";
import { DemandPoller } from "./demand-poller.ts";

const fixture = JSON.parse(readFileSync(new URL("../parity-fixtures/creation/queued-recovery.json", import.meta.url), "utf8"));

test("deferred recovery keeps an empty surface polling until a recovered job terminates", async () => {
  const previousWindow = globalThis.window;
  const timers = new Map<number, () => void>();
  let nextTimer = 0;
  globalThis.window = {
    setTimeout: (callback: () => void) => { timers.set(++nextTimer, callback); return nextTimer; },
    clearTimeout: (id: number) => { timers.delete(id); },
  } as unknown as Window;
  try {
    let pending = true;
    let serverJobs: Array<{ jobId: string; stage: string }> = [];
    let jobs = serverJobs;
    const order: string[] = [];
    const source = new CreationPollState(
      async () => { order.push("pending"); return pending; },
      async () => { order.push("jobs"); return serverJobs; },
    );
    const first = await source.refresh();
    assert.deepEqual(first?.jobs, []);
    assert.deepEqual(order, ["pending", "jobs"]);
    const monitor = new DemandPoller({
      hasWork: () => source.hasWork(jobs.some((job) => job.stage === "queued")),
      poll: async () => { const snapshot = await source.refresh(); if (snapshot) jobs = snapshot.jobs; },
      pollEveryMs: 1,
    });
    monitor.start();
    assert.equal(timers.size, 1);
    const tick = async () => {
      const [id, callback] = timers.entries().next().value!;
      timers.delete(id); callback(); await settle();
    };
    pending = false; serverJobs = [{ jobId: "saved", stage: "queued" }];
    await tick();
    assert.deepEqual(jobs, serverJobs);
    assert.equal(timers.size, 1);
    serverJobs = [{ jobId: "saved", stage: "done" }];
    await tick();
    assert.deepEqual(jobs, serverJobs);
    assert.equal(timers.size, 0);
    monitor.start();
    assert.equal(timers.size, 0);
    monitor.dispose();
  } finally {
    globalThis.window = previousWindow;
  }
});

test("an older reply cannot erase newer terminal or recovery state", async () => {
  let release: (jobs: Array<{ jobId: string; stage: string }>) => void;
  const slow = new Promise<Array<{ jobId: string; stage: string }>>((resolve) => { release = resolve; });
  let calls = 0;
  const source = new CreationPollState(async () => false,
    () => ++calls === 1 ? slow : Promise.resolve([{ jobId: "saved", stage: "failed" }]));
  const old = source.refresh();
  const newest = await source.refresh();
  assert.equal(newest?.reachedTerminal, true);
  release!([{ jobId: "saved", stage: "queued" }]);
  assert.equal(await old, undefined);
  assert.equal(source.hasWork(false), false);
  assert.equal((await source.refresh())?.reachedTerminal, false);
});

test("shared recovery demand cases preserve idle and active behavior", async () => {
  for (const row of fixture.pollingCases) {
    const jobs = row.busyJobs ? [{ jobId: "saved", stage: "queued" }] : [];
    const source = new CreationPollState(async () => row.recoveryPending, async () => jobs);
    const snapshot = await source.refresh();
    assert.equal(source.hasWork(Boolean(snapshot?.jobs.length)), row.expectedPolling);
  }
});
