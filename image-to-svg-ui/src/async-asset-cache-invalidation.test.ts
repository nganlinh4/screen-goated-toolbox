import assert from "node:assert/strict";
import test from "node:test";
import { AsyncAssetCache } from "./async-asset-cache.ts";

function deferred() {
  let resolve!: (value: string) => void;
  const promise = new Promise<string>((done) => { resolve = done; });
  return { promise, resolve };
}

test("a load in flight when the key is invalidated does not re-cache its stale value", async () => {
  const cache = new AsyncAssetCache(4, 1_024);
  const old = deferred();
  const inFlight = cache.load("result.svg", () => old.promise);
  cache.delete("result.svg"); // the file was just saved
  old.resolve("asset://result.svg?v=old");
  await inFlight;

  assert.equal(cache.get("result.svg"), undefined);
  assert.equal(await cache.load("result.svg", async () => "asset://result.svg?v=new"), "asset://result.svg?v=new");
});

test("a newer set wins over an older load that settles later", async () => {
  const cache = new AsyncAssetCache(4, 1_024);
  const old = deferred();
  const inFlight = cache.load("result.svg", () => old.promise);
  cache.set("result.svg", "<svg>saved</svg>");
  old.resolve("<svg>before save</svg>");
  await inFlight;

  assert.equal(cache.get("result.svg"), "<svg>saved</svg>");
});

test("a superseded load does not drop the newer pending request", async () => {
  const cache = new AsyncAssetCache(4, 1_024);
  const old = deferred();
  const fresh = deferred();
  let freshLoads = 0;
  const first = cache.load("k", () => old.promise);
  cache.delete("k");
  const second = cache.load("k", () => { freshLoads += 1; return fresh.promise; });
  old.resolve("old");
  await first;
  const third = cache.load("k", () => { freshLoads += 1; return fresh.promise; });
  fresh.resolve("new");

  assert.equal(await second, "new");
  assert.equal(await third, "new");
  assert.equal(freshLoads, 1);
});
