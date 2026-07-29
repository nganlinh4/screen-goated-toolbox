import assert from "node:assert/strict";
import test from "node:test";
import { deleteSavedResults } from "./history-actions.ts";

test("single and bulk deletion dispatch immediately without a confirmation state", async () => {
  const calls: Array<[string, Record<string, unknown> | undefined]> = [];
  const invoke = async <T>(command: string, args?: Record<string, unknown>): Promise<T> => {
    calls.push([command, args]);
    return undefined as T;
  };

  assert.equal(await deleteSavedResults(invoke, "saved-one"), true);
  assert.equal(await deleteSavedResults(invoke), true);
  assert.deepEqual(calls, [
    ["delete_history_result", { id: "saved-one" }],
    ["delete_all_history_results", {}],
  ]);
});
