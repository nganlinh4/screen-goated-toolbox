import test from "node:test";
import assert from "node:assert/strict";
import { monitorReadiness } from "./readiness-monitor.ts";

test("readiness follows preparing and ready without overlapping queries", async (context) => {
  context.mock.timers.enable({ apis: ["setTimeout"] });
  const seen: string[] = [];
  let state = "preparing";
  let calls = 0;
  const stop = monitorReadiness(async <T>() => { calls++; return state as T; }, value => seen.push(value));
  await Promise.resolve();
  state = "ready";
  context.mock.timers.tick(2_000);
  await Promise.resolve();
  assert.deepEqual(seen, ["preparing", "ready"]);
  stop();
  context.mock.timers.tick(4_000);
  assert.equal(calls, 2);
});

test("disposed readiness ignores a late reply", async () => {
  let resolve!: (value: string) => void;
  const reply = new Promise<string>(done => { resolve = done; });
  const seen: string[] = [];
  const stop = monitorReadiness(async <T>() => await reply as T, value => seen.push(value));
  stop();
  resolve("ready");
  await Promise.resolve();
  await Promise.resolve();
  assert.deepEqual(seen, []);
});
