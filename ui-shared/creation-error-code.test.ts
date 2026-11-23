import assert from "node:assert/strict";
import test from "node:test";
import { creationErrorCode, creationErrorMessageKey } from "./creation-error-code.ts";

test("a coded bridge rejection selects its specific message key", () => {
  assert.equal(creationErrorCode(new Error("name_taken")), "name_taken");
  assert.equal(creationErrorMessageKey(new Error("name_taken"), "renameFailed"), "errorNameTaken");
  assert.equal(creationErrorMessageKey(new Error("result_busy"), "deleteFailed"), "errorResultBusy");
  assert.equal(creationErrorMessageKey("queue_full", "deleteFailed"), "errorQueueFull");
});

test("generic, unknown, or prototype-named failures keep the action message", () => {
  for (const error of [
    new Error("operation_failed"),
    new Error("A file named x.glb already exists."),
    new Error("toString"),
    new Error("__proto__"),
    undefined,
    { message: "name_taken" },
  ]) {
    assert.equal(creationErrorCode(error), "operation_failed");
    assert.equal(creationErrorMessageKey(error, "renameFailed"), "renameFailed");
  }
});
