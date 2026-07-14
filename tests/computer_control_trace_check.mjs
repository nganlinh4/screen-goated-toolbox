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
  const actionIds = new Set([...steps.keys(), ...calls.keys(), ...outcomes.keys()]);
  for (const actionId of actionIds) {
    const stepCount = steps.get(actionId)?.length ?? 0;
    if (stepCount !== 1) {
      fail(`action ${actionId} has ${stepCount} step starts; expected exactly one`);
    }
    const outcomeCount = outcomes.get(actionId)?.length ?? 0;
    if (outcomeCount !== 1) {
      fail(`action ${actionId} has ${outcomeCount} outcomes; expected exactly one`);
    }
    const callCount = calls.get(actionId)?.length ?? 0;
    if (callCount !== 1) {
      fail(`action ${actionId} has ${callCount} tool calls; expected exactly one`);
    }
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

function speechStats(events) {
  const utterances = new Map();
  for (const event of events) {
    const id = event.fields?.utterance_id;
    if (id == null) continue;
    const item = utterances.get(id) ?? {};
    if (event.event === "assistant_audio_chunk" && item.firstAudio == null) {
      item.firstAudio = event.mono_ms;
    }
    if (event.event === "assistant_transcript_delta" && item.firstTranscript == null) {
      item.firstTranscript = event.mono_ms;
    }
    if (event.event === "assistant_playback_started" && item.playback == null) {
      item.playback = event.mono_ms;
    }
    if (event.event === "assistant_playback_completed") item.completed = event.mono_ms;
    if (event.event === "assistant_speech_blocked") item.blocked = true;
    utterances.set(id, item);
  }
  const audible = [...utterances.values()].filter((item) => item.playback != null);
  const transcriptDelays = audible
    .filter((item) => item.firstTranscript != null)
    .map((item) => item.playback - item.firstTranscript);
  const audioDelays = audible
    .filter((item) => item.firstAudio != null)
    .map((item) => item.playback - item.firstAudio);
  return {
    utterances: utterances.size,
    audible: audible.length,
    blocked: [...utterances.values()].filter((item) => item.blocked).length,
    transcriptToPlaybackMaxMs: transcriptDelays.length ? Math.max(...transcriptDelays) : null,
    audioToPlaybackMaxMs: audioDelays.length ? Math.max(...audioDelays) : null,
  };
}

function checkTurnCompletionMapping(events, completions) {
  const transcriptTurns = new Map();
  const completionTurns = new Map();

  for (const { event, index } of events
    .map((event, index) => ({ event, index }))
    .filter(({ event }) => event.event === "user_transcript")) {
    const turnId = event.turn_id;
    if (!Number.isSafeInteger(turnId) || turnId <= 0) {
      fail(`user transcript at event ${index + 1} has invalid turn_id ${String(turnId)}`);
      continue;
    }
    const transcript = transcriptTurns.get(turnId) ?? {
      count: 0,
      lastIndex: index,
    };
    transcript.count += 1;
    transcript.lastIndex = index;
    transcriptTurns.set(turnId, transcript);
  }

  for (const completion of completions) {
    const turnId = completion.event.turn_id;
    if (!Number.isSafeInteger(turnId) || turnId <= 0) {
      fail(
        `accepted completion at event ${completion.index + 1} has invalid turn_id ${String(turnId)}`,
      );
      continue;
    }
    const turnCompletions = completionTurns.get(turnId) ?? [];
    turnCompletions.push(completion);
    completionTurns.set(turnId, turnCompletions);
  }

  for (const [turnId, transcript] of transcriptTurns) {
    const turnCompletions = completionTurns.get(turnId) ?? [];
    if (turnCompletions.length !== 1) {
      fail(
        `user turn ${turnId} has ${turnCompletions.length} accepted completions; expected exactly one`,
      );
      continue;
    }
    if (turnCompletions[0].index <= transcript.lastIndex) {
      fail(
        `user turn ${turnId} completed before its ${transcript.count} transcript event(s) were committed`,
      );
    }
  }

  for (const [turnId, turnCompletions] of completionTurns) {
    if (!transcriptTurns.has(turnId)) {
      fail(
        `turn ${turnId} has ${turnCompletions.length} accepted completion(s) without a user transcript`,
      );
    }
  }
}

function stableSurfaceKey(surface) {
  if (!surface || typeof surface !== "object") return "none";
  if (surface.kind === "browser") {
    return `browser:${surface.tab_id ?? "?"}:${JSON.stringify(surface.document_id ?? null)}`;
  }
  if (surface.kind === "native") {
    return `native:${surface.hwnd ?? "?"}:${surface.pid ?? "?"}:${surface.generation ?? "?"}`;
  }
  return JSON.stringify(surface);
}

function checkObservationChurn(events) {
  const maxRepeatedReads = 5;
  let run = null;
  for (const event of events.filter((item) => item.event === "action_outcome")) {
    const observational =
      event.fields?.execution_ok === true &&
      event.fields?.postcondition?.effect === "observation_or_query";
    if (!observational) {
      run = null;
      continue;
    }
    const key = `${event.fields?.effective_tool ?? "unknown"}|${stableSurfaceKey(event.fields?.post_surface)}`;
    if (run?.key === key) {
      run.count += 1;
    } else {
      run = { key, count: 1, firstAction: event.action_id };
    }
    if (run.count > maxRepeatedReads) {
      fail(
        `actions ${run.firstAction}-${event.action_id} repeat the same successful observation ` +
          `${run.count} times on one unchanged surface; expected a route change or completion`,
      );
      run = null;
    }
  }
}

function checkStrict(events) {
  const errors = events.filter((event) => event.event === "typed_error");
  if (errors.length !== 0) {
    fail(`strict trace has ${errors.length} typed error(s)`);
  }

  for (const setup of events.filter((event) => event.event === "model_setup")) {
    if ((setup.fields?.function_count ?? 0) < 50) {
      fail(`strict model setup exposes only ${setup.fields?.function_count ?? 0} functions`);
    }
    if ((setup.fields?.instruction_bytes ?? Infinity) >= 5000) {
      fail(`strict model setup instruction is ${setup.fields?.instruction_bytes ?? "unknown"} bytes; expected <5000`);
    }
  }

  for (const outcome of events.filter((event) => event.event === "action_outcome")) {
    const requested = outcome.fields?.requested_tool;
    const effective = outcome.fields?.effective_tool;
    if (requested && effective && requested !== effective) {
      fail(`action ${outcome.action_id} silently rerouted ${requested} -> ${effective}`);
    }
  }
  checkObservationChurn(events);

  const completions = events
    .map((event, index) => ({ event, index }))
    .filter(({ event }) =>
      event.event === "turn_summary" &&
      ["done", "model_turn_complete"].includes(event.fields?.outcome),
    );
  if (completions.length === 0) fail("strict trace has no accepted completion");
  checkTurnCompletionMapping(events, completions);
  const terminalOutput = new Set(["assistant_audio_chunk", "assistant_transcript_delta"]);
  const forbiddenEffects = new Set(["tool_call", "step_start"]);
  for (const completion of completions) {
    const nextUser = events.findIndex(
      (event, index) => index > completion.index && event.event === "user_transcript",
    );
    const end = nextUser === -1 ? events.length : nextUser;
    const acceptedTerminal = completion.event.fields?.outcome === "done";
    const opens = events
      .map((event, index) => ({ event, index }))
      .filter(({ event, index }) =>
        index > completion.index && index < end && event.event === "terminal_final_response_opened",
      );
    if (acceptedTerminal && opens.length !== 1) {
      fail(`accepted completion has ${opens.length} terminal response opens; expected exactly one`);
    }
    if (acceptedTerminal && opens[0]?.event.fields?.accepted !== true) {
      fail("accepted completion opened a non-accepted terminal response");
    }
    if (!acceptedTerminal && opens.length !== 0) {
      fail(`non-terminal completion has ${opens.length} terminal response opens`);
    }

    let openIndex = -1;
    let closeIndex = -1;
    if (opens.length === 1) {
      openIndex = opens[0].index;
      const closes = events
        .map((event, index) => ({ event, index }))
        .filter(({ event, index }) =>
          index > completion.index && index < end && event.event === "terminal_final_response_closed",
        );
      if (closes.length !== 1 || closes[0]?.index <= openIndex) {
        fail(`accepted completion has ${closes.length} terminal response closes; expected exactly one`);
      } else {
        closeIndex = closes[0].index;
      }
      if (closes[0]?.event.fields?.accepted !== true) {
        fail("accepted completion closed a non-accepted terminal response");
      }
      if (closes[0]?.event.fields?.reason !== "turn_complete") {
        fail(
          `accepted completion closed as ${closes[0]?.event.fields?.reason ?? "unknown"}; expected turn_complete`,
        );
      }
      const starts = events
        .map((event, index) => ({ event, index }))
        .filter(
          ({ event, index }) =>
            index > completion.index &&
            index < end &&
            event.event === "terminal_final_response_started",
        );
      const startsInside = starts.filter(
        ({ index }) => index > openIndex && index < closeIndex,
      );
      const output = events
        .map((event, index) => ({ event, index }))
        .filter(
          ({ event, index }) =>
            index > openIndex && index < closeIndex && terminalOutput.has(event.event),
        );
      const transcriptChars = output
        .filter(({ event }) => event.event === "assistant_transcript_delta")
        .reduce((sum, { event }) => sum + (event.fields?.char_count ?? 0), 0);
      const audioSamples = output
        .filter(({ event }) => event.event === "assistant_audio_chunk")
        .reduce((sum, { event }) => sum + (event.fields?.received_samples_24k ?? 0), 0);
      if (transcriptChars === 0 && audioSamples < 2400) {
        fail("accepted completion produced no final assistant output");
      }
      if (starts.length !== 1 || startsInside.length !== starts.length) {
        fail(
          `terminal response has ${starts.length} stream starts for ${output.length} output event(s); expected exactly one`,
        );
      }
      if (starts[0]?.event.fields?.accepted !== true) {
        fail("terminal response started without accepted=true");
      }
      const firstMeaningfulOutput = output.find(
        ({ event }) =>
          (event.event === "assistant_transcript_delta" && (event.fields?.char_count ?? 0) > 0) ||
          (event.event === "assistant_audio_chunk" && (event.fields?.received_samples_24k ?? 0) >= 2400),
      );
      if (firstMeaningfulOutput && starts[0]?.index >= firstMeaningfulOutput.index) {
        fail("terminal response output did not occur after its accepted stream start");
      }
    } else if (
      events.some(
        (event, index) =>
          index > completion.index &&
          index < end &&
          ["terminal_final_response_started", "terminal_final_response_closed"].includes(event.event),
      )
    ) {
      fail("terminal response lifecycle appeared without exactly one open");
    }

    for (let index = completion.index + 1; index < end; index += 1) {
      const name = events[index].event;
      if (forbiddenEffects.has(name)) {
        fail(`post-completion ${name} at event ${index + 1}`);
      }
      if (name === "immediate_tool_response_sent") {
        fail(`post-completion tool rejection escaped at event ${index + 1}`);
      }
      if (
        name === "terminal_event_dropped" &&
        events[index].fields?.effectful === true
      ) {
        fail(
          `post-completion ${events[index].fields?.kind ?? "effectful event"} was dropped at event ${index + 1}`,
        );
      }
      if (terminalOutput.has(name) && !(index > openIndex && index < closeIndex)) {
        fail(`assistant output ${name} escaped terminal response at event ${index + 1}`);
      }
    }
  }

  const scripted = events.filter((event) => event.event === "scripted_turn_injected");
  if (scripted.length > 0) {
    if (completions.length !== scripted.length) {
      fail(
        `strict scripted run injected ${scripted.length} turn(s) but recorded ${completions.length} accepted completion(s)`,
      );
    }
    const injectionIndices = events
      .map((event, index) => ({ event, index }))
      .filter(({ event }) => event.event === "scripted_turn_injected")
      .map(({ index }) => index);
    for (let turn = 0; turn < injectionIndices.length; turn += 1) {
      const start = injectionIndices[turn];
      const end = injectionIndices[turn + 1] ?? events.length;
      const userTurns = new Set(
        events
          .filter(
            (event, index) =>
              index > start && index < end && event.event === "user_transcript",
          )
          .map((event) => event.turn_id),
      );
      const completionCount = completions.filter(
        ({ index }) => index > start && index < end,
      ).length;
      if (userTurns.size !== 1 || completionCount !== 1) {
        fail(
          `scripted turn ${turn + 1} has ${userTurns.size} user turn(s) and ${completionCount} accepted completion(s)`,
        );
      }
    }
    const endings = events.filter((event) => event.event === "session_end");
    if (endings.at(-1)?.fields?.reason !== "scripted_complete") {
      fail(`strict scripted run ended as ${endings.at(-1)?.fields?.reason ?? "unknown"}`);
    }
    const sessionEndIndex = events.findLastIndex((event) => event.event === "session_end");
    for (const completion of completions.filter(
      ({ event }) => event.fields?.outcome === "done",
    )) {
      const terminalCloseIndex = events.findIndex(
        (event, index) =>
          index > completion.index &&
          index < sessionEndIndex &&
          event.event === "terminal_final_response_closed",
      );
      if (terminalCloseIndex === -1) {
        fail("strict scripted terminal response did not close before session_end");
      }
    }
  }
}

function main() {
  const strict = process.argv.includes("--strict");
  const input = process.argv.slice(2).find((arg) => !arg.startsWith("--"));
  if (!input) {
    process.stderr.write("Usage: node tests/computer_control_trace_check.mjs [--strict] <events.jsonl>\n");
    process.exit(2);
  }
  const file = path.resolve(input);
  const events = readEvents(file);
  const actions = checkExactlyOnce(events);
  const frames = checkFrames(events);
  checkDelivery(events);
  const lifecycle = checkLifecycle(events);
  const speech = speechStats(events);
  if (strict) checkStrict(events);
  const errors = events.filter((event) => event.event === "typed_error");
  if (!process.exitCode) {
    process.stdout.write(
      `PASS trace ${lifecycle.sessionId}: ${events.length} events, ` +
      `${actions.steps} actions, ${frames.sent}/${frames.ready} frames sent/ready, ` +
        `${lifecycle.setups} setup profile(s), ${errors.length} typed error(s), ` +
        `speech ${speech.audible}/${speech.utterances} audible ` +
        `(max transcript→playback ${speech.transcriptToPlaybackMaxMs ?? "n/a"}ms, ` +
        `audio→playback ${speech.audioToPlaybackMaxMs ?? "n/a"}ms; ${speech.blocked} blocked)\n`,
    );
  }
}

try {
  main();
} catch (error) {
  fail(error.stack ?? error.message);
}
