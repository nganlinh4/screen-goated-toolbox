import assert from "node:assert/strict";
import test from "node:test";
import { ImagePreviewCache, normalizedProjectThumbnail } from "./image-preview-cache.ts";

test("preview requests are shared and cached by path and size", async () => {
  let calls = 0;
  const cache = new ImagePreviewCache(async <T>() => {
    calls += 1;
    return { dataUrl: "data:image/jpeg;base64,preview" } as T;
  });

  const [first, second] = await Promise.all([
    cache.load("source.png", 128),
    cache.load("source.png", 128),
  ]);
  const third = await cache.load("source.png", 128);

  assert.equal(calls, 1);
  assert.equal(first, second);
  assert.equal(second, third);
});

test("project thumbnail hydration never replaces a persisted thumbnail", async () => {
  let resolve!: (value: { dataUrl: string }) => void;
  const cache = new ImagePreviewCache(<T>() => new Promise((done) => {
    resolve = done as (value: { dataUrl: string }) => void;
  }) as Promise<T>);
  const item = { path: "source.png" } as Parameters<typeof cache.ensureProjectThumbnail>[0];
  cache.ensureProjectThumbnail(item, () => undefined);
  item.thumbnailUrl = "persisted";
  resolve({ dataUrl: "generated" });
  await Promise.resolve();

  assert.equal(item.thumbnailUrl, "persisted");
});

test("persisted project thumbnails accept only the bounded JPEG contract", () => {
  assert.equal(normalizedProjectThumbnail("data:image/jpeg;base64,YQ=="), "data:image/jpeg;base64,YQ==");
  assert.equal(normalizedProjectThumbnail("data:image/svg+xml,<svg/>"), undefined);
  assert.equal(normalizedProjectThumbnail(`data:image/jpeg;base64,${"A".repeat(17_000)}`), undefined);
});
