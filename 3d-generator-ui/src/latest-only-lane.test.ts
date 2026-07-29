import assert from "node:assert/strict";
import test from "node:test";
import { LatestOnlyLane } from "./latest-only-lane.ts";

type Deferred<T> = {
  promise: Promise<T>;
  resolve: (value: T) => void;
  reject: (error: Error) => void;
};

function deferred<T>(): Deferred<T> {
  let resolve!: (value: T) => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<T>((accept, decline) => {
    resolve = accept;
    reject = decline;
  });
  return { promise, resolve, reject };
}

async function turn() {
  await Promise.resolve();
  await Promise.resolve();
}

test("only the newest request starts after an active request settles", async () => {
  const lane = new LatestOnlyLane<string>();
  const firstResult = deferred<string>();
  const started: string[] = [];
  const disposed: string[] = [];
  let active = 0;
  let maximumActive = 0;
  const load = (name: string, result: Promise<string>) => lane.run(
    async () => {
      started.push(name);
      active += 1;
      maximumActive = Math.max(maximumActive, active);
      try {
        return await result;
      } finally {
        active -= 1;
      }
    },
    (value) => disposed.push(value),
  );

  const first = load("first", firstResult.promise);
  await turn();
  const middle = load("middle", Promise.resolve("middle-value"));
  const newest = load("newest", Promise.resolve("newest-value"));
  firstResult.resolve("first-value");

  assert.equal(await first, null);
  assert.equal(await middle, null);
  assert.equal(await newest, "newest-value");
  assert.deepEqual(started, ["first", "newest"]);
  assert.deepEqual(disposed, ["first-value"]);
  assert.equal(maximumActive, 1);
});

test("invalidate aborts the active request and disposes a late value", async () => {
  const lane = new LatestOnlyLane<{ id: number }>();
  const result = deferred<{ id: number }>();
  const disposed: number[] = [];
  let observedSignal: AbortSignal | undefined;
  const pending = lane.run(
    async (signal) => {
      observedSignal = signal;
      return result.promise;
    },
    (value) => disposed.push(value.id),
  );
  await turn();

  lane.invalidate();
  assert.equal(observedSignal?.aborted, true);
  result.resolve({ id: 7 });

  assert.equal(await pending, null);
  assert.deepEqual(disposed, [7]);
});

test("a failed request does not poison the serialized lane", async () => {
  const lane = new LatestOnlyLane<string>();
  await assert.rejects(
    lane.run(async () => {
      throw new Error("broken");
    }, () => undefined),
    /broken/,
  );
  assert.equal(await lane.run(async () => "ready", () => undefined), "ready");
});
