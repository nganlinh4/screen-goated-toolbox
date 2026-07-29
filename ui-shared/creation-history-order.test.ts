import assert from "node:assert/strict";
import test from "node:test";
import { newestSessionsFirst } from "./creation-history-order.ts";

test("newestSessionsFirst mixes uploads, active jobs, and saved results by recency", () => {
  const input = [
    { name: "old-result", createdAtMs: 10 },
    { name: "new-upload", createdAtMs: 40 },
    { name: "processing", createdAtMs: 30 },
    { name: "new-result", createdAtMs: 20 },
  ];

  assert.deepEqual(
    newestSessionsFirst(input).map((item) => item.name),
    ["new-upload", "processing", "new-result", "old-result"],
  );
});
