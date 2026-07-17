#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";

const input = process.argv[2];
if (!input) {
  console.error("usage: node tests/computer_control_trace_metrics.mjs <events.jsonl>");
  process.exit(2);
}

const tracePath = path.resolve(input);
const records = fs
  .readFileSync(tracePath, "utf8")
  .split(/\r?\n/)
  .filter(Boolean)
  .map((line, index) => {
    try {
      return JSON.parse(line);
    } catch (error) {
      throw new Error(`invalid JSONL at line ${index + 1}: ${error.message}`);
    }
  });

if (!records.length) throw new Error("trace is empty");

const events = (name) => records.filter((record) => record.event === name);
const field = (record, name) => record?.fields?.[name];
const countBy = (items, value) =>
  Object.fromEntries(
    [...items.reduce((counts, item) => {
      const key = String(value(item) ?? "<missing>");
      counts.set(key, (counts.get(key) ?? 0) + 1);
      return counts;
    }, new Map())].sort(([left], [right]) => left.localeCompare(right)),
  );
const numeric = (items, value) =>
  items.map(value).filter((entry) => Number.isFinite(entry));
const distribution = (values) => {
  if (!values.length) return null;
  const sorted = [...values].sort((left, right) => left - right);
  const percentile = (fraction) => sorted[Math.ceil(fraction * sorted.length) - 1];
  return {
    count: sorted.length,
    min: sorted[0],
    median: percentile(0.5),
    p95: percentile(0.95),
    max: sorted.at(-1),
    total: sorted.reduce((sum, value) => sum + value, 0),
  };
};

const mono = numeric(records, (record) => Number(record.mono_ms));
const turns = events("turn_summary").map((record) => ({
  turn_id: record.turn_id,
  duration_ms: field(record, "duration_ms"),
  outcome: field(record, "outcome"),
  tool_count: field(record, "tool_count"),
  research_count: field(record, "research_count"),
  stall_count: field(record, "stall_count"),
  tools: field(record, "tools") ?? [],
}));
const actions = events("action_outcome");
const typedErrors = events("typed_error");
const sessionEnd = events("session_end").at(-1);

const report = {
  schema_version: 2,
  trace: tracePath,
  session_id: records.find((record) => record.session_id)?.session_id ?? null,
  record_count: records.length,
  elapsed_ms:
    mono.length > 1 ? Math.max(...mono) - Math.min(...mono) : mono.at(0) ?? null,
  session_end_reason: field(sessionEnd, "reason") ?? null,
  turns,
  turn_outcomes: countBy(turns, (turn) => turn.outcome),
  tools: {
    calls: events("tool_call").length,
    results: events("tool_result").length,
    by_name: countBy(events("tool_call"), (record) => field(record, "name")),
    failed_results: events("tool_result").filter(
      (record) => field(record, "ok") === false,
    ).length,
  },
  timing_ms: {
    thinking: distribution(
      numeric(events("think_complete"), (record) => Number(field(record, "duration_ms"))),
    ),
    action_worker: distribution(
      numeric(actions, (record) => Number(field(record, "total_worker_ms"))),
    ),
  },
  effects: {
    may_have_occurred: actions.filter(
      (record) => field(record, "effect_may_have_occurred") === true,
    ).length,
    verified: actions.filter((record) => field(record, "effect_verified") === true).length,
    execution_failed: actions.filter((record) => field(record, "execution_ok") === false)
      .length,
  },
  typed_errors: {
    count: typedErrors.length,
    by_code: countBy(typedErrors, (record) => field(record, "code")),
  },
  frames: {
    ready: events("frame_ready").length,
    sent: events("frame_sent").length,
    surface_bound: events("frame_surface_bound").length,
    vision_requests: events("vision_request").length,
  },
  terminal: {
    opened: events("terminal_final_response_opened").length,
    closed: events("terminal_final_response_closed").length,
    pre_tool_closed: events("terminal_final_response_closed").filter(
      (record) => field(record, "reason") === "pre_tool_response",
    ).length,
    post_tool_closed: events("terminal_final_response_closed").filter(
      (record) => field(record, "reason") === "turn_complete",
    ).length,
    model_boundary_closed: events("model_generation_closed").length,
    cleanup_enqueued: events("turn_cleanup_enqueued").length,
    cleanup_completed: events("turn_cleanup_completed").length,
    assistant_turn_complete: events("assistant_turn_complete").length,
    scripted_turns_injected: events("scripted_turn_injected").length,
  },
};

console.log(JSON.stringify(report, null, 2));
