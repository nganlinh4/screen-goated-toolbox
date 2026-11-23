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

test("a later ambiguous or failed effect invalidates earlier verified progress", () => {
  const verified = event("action_outcome", 4, {
    requested_tool: "click",
    effect_status: "verified",
    effect_verified: true,
  });
  const later = [
    {
      requested_tool: "submit",
      effect_status: "may_have_occurred",
      effect_verified: false,
      effect_may_have_occurred: true,
    },
    {
      requested_tool: "submit",
      effect_status: "proven_no_effect",
      effect_verified: false,
      execution_ok: false,
    },
  ];
  for (const fields of later) {
    const failures = completionEvidenceFailures([
      verified,
      event("action_outcome", 4, fields),
      event("turn_summary", 4, { outcome: "done", task_class: "action" }),
    ]);
    assert.equal(failures.length, 1, JSON.stringify(fields));
  }
});

test("a later verified effect after an ambiguous one supports explicit done", () => {
  const failures = completionEvidenceFailures([
    event("action_outcome", 5, {
      requested_tool: "submit",
      effect_status: "may_have_occurred",
      effect_may_have_occurred: true,
    }),
    event("action_outcome", 5, {
      requested_tool: "submit",
      effect_status: "verified",
      effect_verified: true,
    }),
    event("turn_summary", 5, { outcome: "done", task_class: "action" }),
  ]);
  assert.deepEqual(failures, []);
});

test("a later no-effect read does not invalidate verified progress", () => {
  const failures = completionEvidenceFailures([
    event("action_outcome", 6, {
      requested_tool: "type",
      effect_status: "verified",
      effect_verified: true,
    }),
    event("action_outcome", 6, {
      requested_tool: "inspect",
      effect_status: "proven_no_effect",
      effect_verified: false,
      effect_may_have_occurred: false,
      executed: false,
      ok: true,
    }),
    event("turn_summary", 6, { outcome: "done", task_class: "action" }),
  ]);
  assert.deepEqual(failures, []);
});
