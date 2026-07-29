import assert from "node:assert/strict";
import test from "node:test";
import { PreviewStore } from "./previewStore.ts";

test("reuses stage previews while switching between recent history items", async () => {
  const calls: string[] = [];
  const commands: string[] = [];
  const store = new PreviewStore(async <T>(command: string, args?: Record<string, unknown>) => {
    const path = String(args?.path ?? "");
    calls.push(path);
    commands.push(command);
    assert.deepEqual(Object.keys(args || {}), ["path"]);
    return { url: `preview:${path}` } as T;
  });

  assert.equal(await store.stage("first.png", 1_600), "preview:first.png");
  assert.equal(await store.stage("second.png", 1_600), "preview:second.png");
  assert.equal(await store.stage("first.png", 1_600), "preview:first.png");
  assert.deepEqual(calls, ["first.png", "second.png"]);
  assert.deepEqual(commands, ["image_asset_url", "image_asset_url"]);
});

test("deduplicates simultaneous preview requests", async () => {
  let calls = 0;
  const store = new PreviewStore(async <T>() => {
    calls += 1;
    await Promise.resolve();
    return { url: "preview:shared" } as T;
  });

  const values = await Promise.all([
    store.stage("shared.png", 1_600),
    store.stage("shared.png", 1_600),
  ]);

  assert.deepEqual(values, ["preview:shared", "preview:shared"]);
  assert.equal(calls, 1);
});
