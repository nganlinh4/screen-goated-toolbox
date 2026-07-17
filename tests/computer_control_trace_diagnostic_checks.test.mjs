import assert from "node:assert/strict";
import test from "node:test";

import { diagnosticCompletenessFailures } from "./computer_control_trace_diagnostic_checks.mjs";

test("failed actions require one correlated typed error", () => {
  const events = [
    {
      event: "action_outcome",
      action_id: 7,
      fields: { ok: false, execution_ok: false },
    },
  ];
  assert.deepEqual(diagnosticCompletenessFailures(events), [
    "1 failed action(s) lack exactly one correlated typed error: 7(0)",
  ]);
  events.push({ event: "typed_error", action_id: 7, fields: {} });
  assert.deepEqual(diagnosticCompletenessFailures(events), []);
});

test("tool cycles usage asr and research expose bounded structural diagnostics", () => {
  const events = [
    {
      event: "tool_response_sent",
      action_id: 2,
      fields: { response_byte_count: 80, result_byte_count: 45, generation_index: 1 },
    },
    {
      event: "model_usage",
      fields: { usage_event_index: 1, generation_index: 2 },
    },
    {
      event: "input_transcript_committed",
      fields: {
        source: "provider_input_transcript",
        endpoint_reason: "provider_unspecified",
        finality: "provider_unspecified",
      },
    },
    {
      event: "research_complete",
      fields: { failure_count: 1, diagnostic_codes: ["source_page_unavailable"] },
    },
  ];
  assert.deepEqual(diagnosticCompletenessFailures(events), []);
});

test("missing cycle attribution and redacted research codes are rejected", () => {
  const failures = diagnosticCompletenessFailures([
    { event: "tool_response_sent", action_id: 3, fields: {} },
    { event: "research_complete", fields: { failure_count: 2 } },
  ]);
  assert.equal(failures.length, 2);
  assert.match(failures.join("\n"), /response_byte_count/);
  assert.match(failures.join("\n"), /result_byte_count/);
  assert.match(failures.join("\n"), /generation_index/);
  assert.match(failures.join("\n"), /diagnostic_codes/);
});
