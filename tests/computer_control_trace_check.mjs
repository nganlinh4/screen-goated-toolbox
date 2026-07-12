#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";

function fail(message) {
  process.stderr.write(`FAIL ${message}\n`);
  process.exitCode = 1;
}

function readEvents(file) {
  const lines = fs.readFileSync(file, "utf8").split(/\r?\n/).filter(Boolean);
  return lines.map((line, index) => {
    try {
      return JSON.parse(line);
    } catch (error) {
      throw new Error(`invalid JSONL at line ${index + 1}: ${error.message}`);
    }
  });
}

function grouped(events, eventName) {
  const groups = new Map();
  for (const event of events.filter((item) => item.event === eventName)) {
    const id = event.action_id;
    if (id == null) continue;
    const list = groups.get(id) ?? [];
    list.push(event);
    groups.set(id, list);
  }
  return groups;
}

function checkExactlyOnce(events) {
  const steps = grouped(events, "step_start");
  const calls = grouped(events, "tool_call");
  const outcomes = grouped(events, "action_outcome");
  for (const [actionId] of steps) {
    const outcomeCount = outcomes.get(actionId)?.length ?? 0;
    if (outcomeCount !== 1) {
      fail(`action ${actionId} has ${outcomeCount} outcomes; expected exactly one`);
    }
    const callCount = calls.get(actionId)?.length ?? 0;
    if (callCount !== 1) {
      fail(`action ${actionId} has ${callCount} tool calls; expected exactly one`);
    }
  }
  for (const [actionId] of outcomes) {
    if (!steps.has(actionId)) fail(`outcome ${actionId} has no step_start`);
  }
  return { steps: steps.size, outcomes: outcomes.size };
}

function checkFrames(events) {
  const droppedActions = new Set(
    events
      .filter((event) => event.event === "action_result_dropped")
      .map((event) => event.action_id)
      .filter((id) => id != null),
  );
  const ready = new Set(
    events
      .filter((event) => event.event === "frame_ready")
      .map((event) => event.fields?.frame_id)
      .filter((id) => id != null),
  );
  const sent = new Set(
    events
      .filter((event) => event.event === "frame_sent" && event.fields?.ok === true)
      .map((event) => event.fields?.frame_id)
      .filter((id) => id != null),
  );
  for (const frameId of sent) {
    if (!ready.has(frameId)) fail(`sent frame ${frameId} has no frame_ready record`);
  }
  for (const event of events.filter((item) => item.event === "action_outcome")) {
    const source = event.fields?.source_frame_id;
    const post = event.fields?.post_frame_id;
    if (source != null && !sent.has(source)) {
      fail(`action ${event.action_id} source frame ${source} was never sent`);
    }
    if (post != null && !ready.has(post)) {
      fail(`action ${event.action_id} post frame ${post} lacks ready provenance`);
    }
    if (post != null && !droppedActions.has(event.action_id) && !sent.has(post)) {
      fail(`action ${event.action_id} post frame ${post} was not sent`);
    }
    if (post != null && droppedActions.has(event.action_id) && sent.has(post)) {
      fail(`dropped action ${event.action_id} sent stale post frame ${post}`);
    }
  }
  return { ready: ready.size, sent: sent.size };
}

function checkDelivery(events) {
  const delivered = new Set(
    events
      .filter((event) => ["tool_response_sent", "action_result_dropped"].includes(event.event))
      .map((event) => event.action_id)
      .filter((id) => id != null),
  );
  for (const outcome of events.filter(
    (event) => event.event === "action_outcome" && event.fields?.executed === true,
  )) {
    if (!delivered.has(outcome.action_id)) {
      fail(`executed action ${outcome.action_id} has no response-send or drop record`);
    }
  }
}

function checkLifecycle(events) {
  if (!events.some((event) => event.event === "session_start")) fail("missing session_start");
  if (!events.some((event) => event.event === "session_end")) fail("missing session_end");
  const setups = events.filter((event) => event.event === "model_setup");
  if (setups.length === 0) fail("missing model_setup");
  for (const setup of setups) {
    if (!(setup.fields?.function_count > 0) || !(setup.fields?.setup_bytes > 0)) {
      fail("model_setup is missing catalog/size data");
    }
  }
  const sessions = new Set(events.map((event) => event.session_id));
  if (sessions.size !== 1) fail(`trace contains ${sessions.size} session IDs`);
  const monotonic = events.every(
    (event, index) => index === 0 || event.mono_ms >= events[index - 1].mono_ms,
  );
  if (!monotonic) fail("mono_ms is not monotonic");
  return { setups: setups.length, sessionId: [...sessions][0] };
}

function main() {
  const input = process.argv[2];
  if (!input) {
    process.stderr.write("Usage: node tests/computer_control_trace_check.mjs <events.jsonl>\n");
    process.exit(2);
  }
  const file = path.resolve(input);
  const events = readEvents(file);
  const actions = checkExactlyOnce(events);
  const frames = checkFrames(events);
  checkDelivery(events);
  const lifecycle = checkLifecycle(events);
  const errors = events.filter((event) => event.event === "typed_error");
  if (!process.exitCode) {
    process.stdout.write(
      `PASS trace ${lifecycle.sessionId}: ${events.length} events, ` +
        `${actions.steps} actions, ${frames.sent}/${frames.ready} frames sent/ready, ` +
        `${lifecycle.setups} setup profile(s), ${errors.length} typed error(s)\n`,
    );
  }
}

try {
  main();
} catch (error) {
  fail(error.stack ?? error.message);
}
