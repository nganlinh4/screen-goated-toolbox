import assert from "node:assert/strict";
import test from "node:test";
import { AsyncAssetCache } from "./async-asset-cache.ts";

test("reuses recent assets and deduplicates concurrent loads", async () => {
  const cache = new AsyncAssetCache(4, 1_024);
  let loads = 0;
  const loader = async () => {
    loads += 1;
    return "preview";
  };

  const [first, second] = await Promise.all([
    cache.load("same", loader),
    cache.load("same", loader),
  ]);
  const third = await cache.load("same", loader);

  assert.equal(first, "preview");
  assert.equal(second, "preview");
  assert.equal(third, "preview");
  assert.equal(loads, 1);
});

test("bounds retained assets by count and weight", () => {
  const cache = new AsyncAssetCache(2, 7);
  cache.set("first", "111");
  cache.set("second", "22");
  cache.get("first");
  cache.set("third", "333");

  assert.equal(cache.get("second"), undefined);
  assert.equal(cache.get("first"), "111");
  assert.equal(cache.get("third"), "333");

  cache.set("large", "12345678");
  assert.equal(cache.get("large"), undefined);
  assert.equal(cache.get("first"), undefined);
});
