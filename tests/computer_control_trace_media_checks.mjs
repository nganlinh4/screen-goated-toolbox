export function checkSpeechRetirement(events, fail) {
  const maxAudioToPlaybackMs = 2000;
  const maxTranscriptToPlaybackMs = 5000;
  const utterances = new Map();
  const itemFor = (id) => {
    const item = utterances.get(id) ?? {
      firstAudio: null,
      firstSinkAudio: null,
      firstPlayableAudio: null,
      firstTranscript: null,
      playbackStarts: [],
      playbackCompletions: [],
      interruptions: [],
      playableOutputSamples: 0,
      discarded: false,
      outcomeHeldSamples: 0,
    };
    utterances.set(id, item);
    return item;
  };

  for (const event of events) {
    const id = event.fields?.utterance_id;
    if (!Number.isSafeInteger(id) || id <= 0) continue;
    const item = itemFor(id);
    if (event.event === "assistant_audio_chunk") {
      if (item.firstAudio == null) item.firstAudio = event.mono_ms;
      if (event.fields?.audio_sink_available === true) {
        if (item.firstSinkAudio == null) item.firstSinkAudio = event.mono_ms;
        if (event.fields?.held_for_generation_outcome === true) {
          item.outcomeHeldSamples += event.fields?.received_samples_24k ?? 0;
        }
      }
      if (
        event.fields?.audio_sink_available === true &&
        event.fields?.held_for_generation_outcome === false
      ) {
        if (item.firstPlayableAudio == null) item.firstPlayableAudio = event.mono_ms;
        item.playableOutputSamples += event.fields?.queued_from_chunk ?? 0;
      }
    } else if (event.event === "assistant_generation_audio_released") {
      const queued =
        (event.fields?.queued_output_samples ?? 0) +
        (event.fields?.flushed_startup_samples ?? 0);
      if (queued > 0) {
        if (item.firstPlayableAudio == null) item.firstPlayableAudio = event.mono_ms;
        item.playableOutputSamples += queued;
      }
    } else if (event.event === "assistant_audio_generation_complete") {
      const queued = event.fields?.flushed_startup_samples ?? 0;
      if (queued > 0) {
        if (item.firstPlayableAudio == null) item.firstPlayableAudio = event.mono_ms;
        item.playableOutputSamples += queued;
      }
    } else if (event.event === "assistant_generation_audio_discarded") {
      item.discarded = true;
    } else if (event.event === "assistant_transcript_delta") {
      if (item.firstTranscript == null) item.firstTranscript = event.mono_ms;
    } else if (event.event === "assistant_playback_started") {
      item.playbackStarts.push(event.mono_ms);
    } else if (event.event === "assistant_playback_completed") {
      item.playbackCompletions.push(event.mono_ms);
    } else if (event.event === "assistant_playback_interrupted") {
      item.interruptions.push(event.mono_ms);
    }
  }

  for (const [id, item] of utterances) {
    if (item.outcomeHeldSamples > 0) {
      fail(
        `assistant utterance ${id} held ${item.outcomeHeldSamples} samples for a semantic ` +
          "outcome despite an available audio sink",
      );
    }
    const ownsPlayableAudio = item.playableOutputSamples > 0;
    if (ownsPlayableAudio && item.playbackStarts.length !== 1) {
      fail(
        `assistant utterance ${id} has ${item.playbackStarts.length} playback starts; expected one`,
      );
    }
    if (item.playbackStarts.length > 1) {
      fail(`assistant utterance ${id} restarted playback ${item.playbackStarts.length} times`);
    }
    if (item.discarded && !ownsPlayableAudio && item.playbackStarts.length !== 0) {
      fail(`discarded-only assistant utterance ${id} reached playback`);
    }
    if (item.playbackCompletions.length > 1) {
      fail(
        `assistant utterance ${id} completed playback ${item.playbackCompletions.length} times`,
      );
    }
    const retired = item.playbackCompletions.length + item.interruptions.length;
    if (item.playbackStarts.length === 1 && retired !== 1) {
      fail(
        `assistant utterance ${id} has ${retired} playback retirements; expected exactly one`,
      );
    }
    const started = item.playbackStarts[0];
    if (started != null && item.firstSinkAudio != null) {
      const delay = started - item.firstSinkAudio;
      if (delay > maxAudioToPlaybackMs) {
        fail(
          `assistant utterance ${id} waited ${delay}ms from sink-eligible audio to playback ` +
            `(limit ${maxAudioToPlaybackMs}ms)`,
        );
      }
    }
    if (started != null && item.firstPlayableAudio != null) {
      const delay = started - item.firstPlayableAudio;
      if (delay > maxAudioToPlaybackMs) {
        fail(
          `assistant utterance ${id} waited ${delay}ms from playable audio to playback ` +
            `(limit ${maxAudioToPlaybackMs}ms)`,
        );
      }
    }
    if (started != null && item.firstTranscript != null) {
      const delay = started - item.firstTranscript;
      if (delay > maxTranscriptToPlaybackMs) {
        fail(
          `assistant utterance ${id} waited ${delay}ms from transcript to playback ` +
            `(limit ${maxTranscriptToPlaybackMs}ms)`,
        );
      }
    }
  }
}

export function checkTurnTabRetirement(events, allowPersistentTabs, fail) {
  const calls = new Map();
  const payloads = new Map();
  const successfulActions = new Set(
    events
      .filter(
        (event) =>
          event.event === "action_outcome" &&
          ["browser_open_tab", "open_url"].includes(event.fields?.effective_tool) &&
          event.fields?.execution_ok === true,
      )
      .map((event) => event.action_id),
  );
  for (const event of events.filter(
    (item) =>
      item.event === "tool_call" &&
      ["browser_open_tab", "open_url"].includes(item.fields?.name),
  )) {
    const callId = event.fields?.tool_call_id;
    if (typeof callId === "string") {
      calls.set(callId, { actionId: event.action_id, turnId: event.turn_id });
    }
  }
  for (const event of events.filter(
    (item) =>
      item.event === "tool_call_payload" &&
      ["browser_open_tab", "open_url"].includes(item.fields?.name),
  )) {
    const callId = event.fields?.tool_call_id;
    if (typeof callId === "string") payloads.set(callId, event.fields?.args ?? {});
  }

  const expectedByTurn = new Map();
  let successfulPersistent = 0;
  for (const [callId, call] of calls) {
    if (!successfulActions.has(call.actionId)) continue;
    const lifetime = payloads.get(callId)?.lifetime ?? "persistent";
    if (lifetime === "persistent") {
      successfulPersistent += 1;
      continue;
    }
    if (lifetime !== "turn") {
      fail(`browser_open_tab action ${call.actionId} has invalid lifetime ${JSON.stringify(lifetime)}`);
      continue;
    }
    expectedByTurn.set(call.turnId, (expectedByTurn.get(call.turnId) ?? 0) + 1);
  }
  if (!allowPersistentTabs && successfulPersistent > 0) {
    fail(
      `strict run opened ${successfulPersistent} persistent browser tab(s); ` +
        "pass --allow-persistent-tabs only when the oracle requires that final state",
    );
  }

  const reportsByTurn = new Map();
  for (const event of events.filter((item) => item.event === "browser_turn_tabs_retired")) {
    const turnId = event.fields?.turn_id ?? event.turn_id;
    const reports = reportsByTurn.get(turnId) ?? [];
    reports.push(event.fields ?? {});
    reportsByTurn.set(turnId, reports);
  }
  for (const [turnId, expected] of expectedByTurn) {
    const reports = reportsByTurn.get(turnId) ?? [];
    const attempted = reports.reduce((sum, report) => sum + (report.attempted ?? 0), 0);
    const closed = reports.reduce((sum, report) => sum + (report.closed_verified ?? 0), 0);
    const preserved = reports.reduce((sum, report) => sum + (report.preserved ?? 0), 0);
    const failed = reports.reduce((sum, report) => sum + (report.failed ?? 0), 0);
    const restoreFailed = reports.reduce(
      (sum, report) => sum + (report.restore_failed ?? 0),
      0,
    );
    if (attempted !== expected) {
      fail(`turn ${turnId} retired ${attempted}/${expected} successful turn-owned browser tabs`);
    }
    if (closed !== expected || preserved !== 0 || failed !== 0 || restoreFailed !== 0) {
      fail(
        `turn ${turnId} tab retirement closed=${closed}, preserved=${preserved}, ` +
          `failed=${failed}, restore_failed=${restoreFailed}; expected ${expected}/0/0/0`,
      );
    }
  }
}
