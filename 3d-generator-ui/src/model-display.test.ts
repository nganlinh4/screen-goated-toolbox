import assert from "node:assert/strict";
import test from "node:test";
import { ModelDisplayLane } from "./model-display.ts";

type ResolveModel = (value: { vertices: number; faces: number } | null) => void;

async function turn() {
  await Promise.resolve();
  await Promise.resolve();
}

test("rapid selections issue and parse only the first active and newest model", async () => {
  const issued: string[] = [];
  const pending = new Map<string, ResolveModel>();
  let cancelled = 0;
  const viewer = {
    cancelPendingModelLoad() {
      cancelled += 1;
      for (const resolve of pending.values()) resolve(null);
      pending.clear();
    },
    setModel(url: string) {
      if (url === "asset:newest.glb") {
        return Promise.resolve({ vertices: 12, faces: 4 });
      }
      return new Promise<{ vertices: number; faces: number } | null>((resolve) => {
        pending.set(url, resolve);
      });
    },
    showIdle() {},
  };
  const display = new ModelDisplayLane(viewer, async <T>(command: string, args?: unknown) => {
    if (command === "release_model_asset") return undefined as T;
    const path = (args as { path: string }).path;
    issued.push(path);
    return { url: `asset:${path}` } as T;
  });

  const first = display.display("first.glb", false);
  await turn();
  const middle = display.display("middle.glb", false);
  const newest = display.display("newest.glb", true);

  assert.equal(await first, null);
  assert.equal(await middle, null);
  assert.deepEqual(await newest, {
    kind: "model",
    stats: { vertices: 12, faces: 4 },
  });
  assert.deepEqual(issued, ["first.glb", "newest.glb"]);
  assert.equal(cancelled, 1);
});

test("switching to an empty session waits out host issuance and never parses stale data", async () => {
  let resolveAsset!: (value: { url: string }) => void;
  const asset = new Promise<{ url: string }>((resolve) => {
    resolveAsset = resolve;
  });
  let parsed = 0;
  let idle = 0;
  let releases = 0;
  const viewer = {
    cancelPendingModelLoad() {},
    async setModel() {
      parsed += 1;
      return { vertices: 1, faces: 1 };
    },
    showIdle() {
      idle += 1;
    },
  };
  const display = new ModelDisplayLane(viewer, async <T>(command: string) => {
    if (command === "release_model_asset") {
      releases += 1;
      return undefined as T;
    }
    return asset as T;
  });

  const stale = display.display("stale.glb", false);
  await turn();
  const empty = display.display(undefined, false);
  resolveAsset({ url: "asset:stale" });

  assert.equal(await stale, null);
  assert.deepEqual(await empty, { kind: "idle" });
  assert.equal(parsed, 0);
  assert.equal(idle, 1);
  assert.equal(releases, 2);
});
