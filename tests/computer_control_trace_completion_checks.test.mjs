import assert from "node:assert/strict";
import test from "node:test";

import { completionEvidenceFailures } from "./computer_control_trace_completion_checks.mjs";

function event(name, turnId, fields = {}) {
  return { event: name, turn_id: turnId, fields };
}

test("explicit done needs a verified action receipt", () => {
  const failures = completionEvidenceFailures([
    event("action_outcome", 1, {
      requested_tool: "act",
      effect_status: "proven_no_effect",
      effect_verified: false,
    }),
    event("turn_summary", 1, { outcome: "done", task_class: "action" }),
  ]);
  assert.equal(failures.length, 1);
});

test("verified progress supports explicit done", () => {
  const failures = completionEvidenceFailures([
    event("action_outcome", 2, {
      requested_tool: "future_action",
      effect_status: "verified",
      effect_verified: true,
    }),
    event("turn_summary", 2, { outcome: "done", task_class: "action" }),
  ]);
  assert.deepEqual(failures, []);
});

test("ordinary model boundary is not treated as an explicit completion claim", () => {
  const failures = completionEvidenceFailures([
    event("turn_summary", 3, {
      outcome: "model_turn_complete",
      task_class: "action",
    }),
  ]);
  assert.deepEqual(failures, []);
});
