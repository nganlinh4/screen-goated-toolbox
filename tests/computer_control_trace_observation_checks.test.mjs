import assert from "node:assert/strict";
import test from "node:test";

import { observationChurnFailures } from "./computer_control_trace_observation_checks.mjs";

function read(action, path) {
  return [
    {
      event: "tool_call_payload",
      action_id: action,
      fields: { name: "read_text_file", args: { path } },
    },
    {
      event: "action_outcome",
      action_id: action,
      fields: {
        effective_tool: "read_text_file",
        execution_ok: true,
        post_surface: null,
        postcondition: { effect: "observation_or_query" },
      },
    },
  ];
}

test("distinct structured resources are not observation churn", () => {
  const events = Array.from({ length: 8 }, (_, index) =>
    read(index + 1, `C:\\fixtures\\record-${index + 1}.txt`),
  ).flat();
  assert.deepEqual(observationChurnFailures(events), []);
});

test("repeating one identical structured observation is still bounded", () => {
  const events = Array.from({ length: 6 }, (_, index) =>
    read(index + 1, "C:\\fixtures\\same.txt"),
  ).flat();
  const failures = observationChurnFailures(events);
  assert.equal(failures.length, 1);
  assert.match(failures[0], /repeat the same successful observation/u);
});
