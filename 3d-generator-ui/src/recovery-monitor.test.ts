import test from "node:test";
import assert from "node:assert/strict";
import { monitorRecovery } from "./recovery-monitor.ts";

const flush = () => new Promise<void>((resolve) => setImmediate(resolve));

test("discovers again after an initially empty pending recovery, and stops on completion", async () => {
  let queued: (() => void) | undefined;
  const timers = {
    set: ((callback: () => void) => { queued = callback; return 1; }) as typeof globalThis.setTimeout,
    clear: (() => { queued = undefined; }) as typeof globalThis.clearTimeout,
  };
  let calls = 0;
  const discoveries: string[] = [];
  const stop = monitorRecovery(
    async () => ++calls === 1,
    async () => { discoveries.push(calls === 1 ? "empty" : "restored"); return true; },
    timers,
  );
  await flush();
  assert.deepEqual(discoveries, ["empty"]);
  assert.ok(queued);
  const next = queued;
  queued = undefined;
  next();
  await flush();
  assert.deepEqual(discoveries, ["empty", "restored"]);
  assert.equal(queued, undefined);
  stop();
});

test("a transient discovery failure retries and page shutdown cancels it", async () => {
  let queued: (() => void) | undefined;
  const timers = {
    set: ((callback: () => void) => { queued = callback; return 1; }) as typeof globalThis.setTimeout,
    clear: (() => { queued = undefined; }) as typeof globalThis.clearTimeout,
  };
  const stop = monitorRecovery(async () => false, async () => false, timers);
  await flush();
  assert.ok(queued);
  stop();
  assert.equal(queued, undefined);
});
