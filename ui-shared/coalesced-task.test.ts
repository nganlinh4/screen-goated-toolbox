import assert from "node:assert/strict";
import test from "node:test";
import { CoalescedTask } from "./coalesced-task.ts";

function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>((done) => { resolve = done; });
  return { promise, resolve };
}

test("a request made while the task runs gets one more run that sees newer data", async () => {
  let source = "before publication";
  const seen: string[] = [];
  const gates = [deferred(), deferred()];
  let calls = 0;
  const task = new CoalescedTask(async () => {
    const snapshot = source;
    await gates[calls++].promise;
    seen.push(snapshot);
  });
  const first = task.run();
  source = "after publication";
  const second = task.run();
  const third = task.run();
  gates[0].resolve();
  await Promise.resolve();
  await new Promise((done) => setImmediate(done));
  gates[1].resolve();
  await Promise.all([first, second, third]);
  assert.deepEqual(seen, ["before publication", "after publication"]);
  assert.equal(calls, 2, "requests made during one run coalesce into a single rerun");
});

test("awaiting a coalesced request waits for the rerun that covers it", async () => {
  const gate = deferred();
  let runs = 0;
  const task = new CoalescedTask(async () => {
    runs += 1;
    if (runs === 1) await gate.promise;
  });
  void task.run();
  let settled = false;
  const later = task.run().then(() => { settled = true; });
  await new Promise((done) => setImmediate(done));
  assert.equal(settled, false);
  gate.resolve();
  await later;
  assert.equal(runs, 2);
});

test("an idle task runs once per request and recovers after a failure", async () => {
  let runs = 0;
  const task = new CoalescedTask(async () => {
    runs += 1;
    if (runs === 1) throw new Error("storage unavailable");
  });
  await assert.rejects(task.run(), /storage unavailable/);
  await task.run();
  assert.equal(runs, 2);
});
