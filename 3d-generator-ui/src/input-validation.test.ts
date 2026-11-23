import test from "node:test";
import { readFileSync } from "node:fs";
import assert from "node:assert/strict";
import { validateInput } from "./input-validation.ts";

test("input preflight forwards the selected mode and preserves public rejection codes", async () => {
  for (const error of [null, "image_too_small", "image_too_large", "image_invalid"] as const) {
    const invoke = async <T>(cmd: string, args?: unknown): Promise<T> => {
      assert.equal(cmd, "validate_image");
      assert.deepEqual(args, { path: "source.png", generationMode: "quality" });
      return { error } as T;
    };
    assert.equal(await validateInput(invoke, "source.png", "quality"), error);
  }
});

test("missing, malformed and unavailable validation fails closed", async () => {
  for (const value of [null, {}, { error: "unknown" }]) {
    assert.equal(await validateInput(async <T>() => value as T, "source.png", "quality"), "validation_unavailable");
  }
  assert.equal(await validateInput(async () => { throw new Error("unavailable"); }, "source.png", "quality"), "validation_unavailable");
});

test("a retired generation mode refused by the host fails closed with the shared error", async () => {
  const fixture = JSON.parse(readFileSync(new URL("../../parity-fixtures/image-to-3d/input-contract.json", import.meta.url), "utf8"));
  for (const row of fixture.retiredModes) {
    const refused = async <T>(cmd: string, args?: unknown): Promise<T> => {
      assert.equal(cmd, "validate_image");
      assert.deepEqual(args, { path: "source.png", generationMode: row.mode });
      throw new Error("Invalid generation mode");
    };
    assert.equal(await validateInput(refused, "source.png", row.mode), row.error);
  }
});
