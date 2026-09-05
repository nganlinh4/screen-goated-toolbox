import test from "node:test";
import assert from "node:assert/strict";
import { validateInput } from "./input-validation.ts";

test("input preflight forwards the selected mode and preserves public rejection codes", async () => {
  for (const error of [null, "image_too_small", "image_too_large", "image_invalid"] as const) {
    const invoke = async <T>(cmd: string, args?: unknown): Promise<T> => {
      assert.equal(cmd, "validate_image");
      assert.deepEqual(args, { path: "source.png", generationMode: "fast" });
      return { error } as T;
    };
    assert.equal(await validateInput(invoke, "source.png", "fast"), error);
  }
});

test("missing, malformed and unavailable validation fails closed", async () => {
  for (const value of [null, {}, { error: "unknown" }]) {
    assert.equal(await validateInput(async <T>() => value as T, "source.png", "quality"), "image_invalid");
  }
  assert.equal(await validateInput(async () => { throw new Error("unavailable"); }, "source.png", "fast"), "image_invalid");
});
