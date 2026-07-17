import assert from "node:assert/strict";
import test from "node:test";

import { checkTerminalResponses } from "./computer_control_trace_terminal_checks.mjs";

function failuresFor(events) {
  const failures = [];
  const completions = events
    .map((event, index) => ({ event, index }))
    .filter(({ event }) => event.event === "turn_summary");
  checkTerminalResponses(events, completions, (message) => failures.push(message));
  return failures;
}

test("speech delivered before done closes without a second generation", () => {
  const events = [
    { event: "turn_summary", fields: { outcome: "done" } },
    {
      event: "terminal_final_response_closed",
      fields: {
        accepted: true,
        reason: "pre_tool_response",
        response_completed: true,
      },
    },
  ];
  assert.deepEqual(failuresFor(events), []);
});

test("silent done owns exactly one final response", () => {
  const events = [
    { event: "turn_summary", fields: { outcome: "done" } },
    { event: "terminal_final_response_opened", fields: { accepted: true } },
    { event: "terminal_final_response_started", fields: { accepted: true } },
    { event: "assistant_transcript_delta", fields: { char_count: 9 } },
    {
      event: "terminal_final_response_closed",
      fields: { accepted: true, reason: "turn_complete" },
    },
  ];
  assert.deepEqual(failuresFor(events), []);
});

test("assistant output after the terminal close is rejected", () => {
  const events = [
    { event: "turn_summary", fields: { outcome: "done" } },
    { event: "terminal_final_response_opened", fields: { accepted: true } },
    { event: "terminal_final_response_started", fields: { accepted: true } },
    { event: "assistant_transcript_delta", fields: { char_count: 9 } },
    {
      event: "terminal_final_response_closed",
      fields: { accepted: true, reason: "turn_complete" },
    },
    { event: "assistant_transcript_delta", fields: { char_count: 7 } },
  ];
  assert.match(failuresFor(events).join("\n"), /escaped terminal response/u);
});

test("ordinary model boundary needs no synthetic terminal lifecycle", () => {
  const events = [
    {
      event: "turn_summary",
      fields: { outcome: "model_turn_complete" },
    },
  ];
  assert.deepEqual(failuresFor(events), []);
});
