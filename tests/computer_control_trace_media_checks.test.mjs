import assert from "node:assert/strict";
import test from "node:test";

import { checkSpeechRetirement } from "./computer_control_trace_media_checks.mjs";

const event = (name, monoMs, fields) => ({
  event: name,
  mono_ms: monoMs,
  fields: { utterance_id: 1, ...fields },
});

function failuresFor(events) {
  const failures = [];
  checkSpeechRetirement(events, (message) => failures.push(message));
  return failures;
}

test("audio held only for a missing sink does not require playback", () => {
  const events = [
    event("assistant_audio_chunk", 100, {
      audio_sink_available: false,
      held_for_generation_outcome: false,
      held_for_sink_recovery: true,
      received_samples_24k: 24_000,
      queued_from_chunk: 0,
    }),
    event("assistant_generation_audio_discarded", 5_000, {
      dropped_samples_24k: 24_000,
    }),
  ];

  assert.deepEqual(failuresFor(events), []);
});

test("streamed generation audio starts promptly and retires once", () => {
  const events = [
    event("assistant_audio_chunk", 100, {
      audio_sink_available: true,
      held_for_generation_outcome: false,
      received_samples_24k: 24_000,
      queued_from_chunk: 48_000,
    }),
    event("assistant_playback_started", 200, {}),
    event("assistant_playback_completed", 1_200, {}),
  ];

  assert.deepEqual(failuresFor(events), []);
});

test("a short streamed utterance becomes playable when generation completes", () => {
  const events = [
    event("assistant_audio_chunk", 100, {
      audio_sink_available: true,
      held_for_generation_outcome: false,
      received_samples_24k: 1_000,
      queued_from_chunk: 0,
    }),
    event("assistant_audio_generation_complete", 130, {
      queued_output_samples: 1_000,
      flushed_startup_samples: 1_000,
    }),
    event("assistant_playback_started", 150, {}),
    event("assistant_playback_completed", 500, {}),
  ];

  assert.deepEqual(failuresFor(events), []);
});

test("whole-generation semantic audio holding is rejected", () => {
  const events = [
    event("assistant_transcript_delta", 100, {}),
    event("assistant_audio_chunk", 100, {
      audio_sink_available: true,
      held_for_generation_outcome: true,
      received_samples_24k: 24_000,
      queued_from_chunk: 0,
    }),
    event("assistant_generation_audio_released", 6_000, {
      queued_output_samples: 48_000,
    }),
    event("assistant_playback_started", 6_100, {}),
    event("assistant_playback_completed", 7_100, {}),
  ];

  assert.match(failuresFor(events).join("\n"), /semantic outcome|sink-eligible|transcript/);
});

test("discarded-only generation audio must never reach playback", () => {
  const events = [
    event("assistant_audio_chunk", 100, {
      audio_sink_available: false,
      held_for_generation_outcome: false,
      held_for_sink_recovery: true,
      received_samples_24k: 24_000,
      queued_from_chunk: 0,
    }),
    event("assistant_generation_audio_discarded", 200, {
      dropped_samples_24k: 24_000,
    }),
    event("assistant_playback_started", 300, {}),
    event("assistant_playback_completed", 1_300, {}),
  ];

  assert.match(failuresFor(events).join("\n"), /discarded-only/);
});
