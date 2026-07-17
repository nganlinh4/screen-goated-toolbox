export function checkTerminalResponses(events, completions, fail) {
  const terminalOutput = new Set([
    "assistant_audio_chunk",
    "assistant_transcript_delta",
  ]);
  const forbiddenEffects = new Set(["tool_call", "step_start"]);

  for (const completion of completions) {
    const nextUser = events.findIndex(
      (event, index) =>
        index > completion.index && event.event === "user_transcript",
    );
    const end = nextUser === -1 ? events.length : nextUser;
    const window = events
      .map((event, index) => ({ event, index }))
      .filter(({ index }) => index > completion.index && index < end);
    const acceptedTerminal = completion.event.fields?.outcome === "done";
    const opens = window.filter(
      ({ event }) => event.event === "terminal_final_response_opened",
    );
    const starts = window.filter(
      ({ event }) => event.event === "terminal_final_response_started",
    );
    const closes = window.filter(
      ({ event }) => event.event === "terminal_final_response_closed",
    );

    let openIndex = -1;
    let closeIndex = -1;
    let ownsPostCompletionOutput = false;
    if (!acceptedTerminal) {
      if (opens.length !== 0 || starts.length !== 0 || closes.length !== 0) {
        fail("non-terminal completion opened a terminal response lifecycle");
      }
    } else if (opens.length === 0) {
      if (starts.length !== 0 || closes.length !== 1) {
        fail(
          `pre-tool completion has ${starts.length} stream starts and ${closes.length} closes; expected 0/1`,
        );
      }
      if (closes[0]?.event.fields?.accepted !== true) {
        fail("pre-tool completion closed a non-accepted terminal response");
      }
      if (
        closes[0]?.event.fields?.reason !== "pre_tool_response" ||
        closes[0]?.event.fields?.response_completed !== true
      ) {
        fail(
          `completion without an open closed as ${closes[0]?.event.fields?.reason ?? "unknown"}; expected completed pre_tool_response`,
        );
      }
      closeIndex = closes[0]?.index ?? -1;
    } else if (opens.length === 1) {
      ownsPostCompletionOutput = true;
      openIndex = opens[0].index;
      if (opens[0]?.event.fields?.accepted !== true) {
        fail("accepted completion opened a non-accepted terminal response");
      }
      if (closes.length !== 1 || closes[0]?.index <= openIndex) {
        fail(
          `accepted completion has ${closes.length} terminal response closes; expected exactly one`,
        );
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

      const startsInside = starts.filter(
        ({ index }) => index > openIndex && index < closeIndex,
      );
      const output = window.filter(
        ({ event, index }) =>
          index > openIndex &&
          index < closeIndex &&
          terminalOutput.has(event.event),
      );
      const transcriptChars = output
        .filter(({ event }) => event.event === "assistant_transcript_delta")
        .reduce(
          (sum, { event }) => sum + (event.fields?.char_count ?? 0),
          0,
        );
      const audioSamples = output
        .filter(({ event }) => event.event === "assistant_audio_chunk")
        .reduce(
          (sum, { event }) =>
            sum + (event.fields?.received_samples_24k ?? 0),
          0,
        );
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
          (event.event === "assistant_transcript_delta" &&
            (event.fields?.char_count ?? 0) > 0) ||
          (event.event === "assistant_audio_chunk" &&
            (event.fields?.received_samples_24k ?? 0) >= 2400),
      );
      if (
        firstMeaningfulOutput &&
        starts[0]?.index >= firstMeaningfulOutput.index
      ) {
        fail(
          "terminal response output did not occur after its accepted stream start",
        );
      }
    } else {
      fail(
        `accepted completion has ${opens.length} terminal response opens; expected at most one`,
      );
    }

    for (const { event, index } of window) {
      const name = event.event;
      if (forbiddenEffects.has(name)) {
        fail(`post-completion ${name} at event ${index + 1}`);
      }
      if (name === "immediate_tool_response_sent") {
        fail(`post-completion tool rejection escaped at event ${index + 1}`);
      }
      if (
        name === "terminal_event_dropped" &&
        event.fields?.effectful === true
      ) {
        fail(
          `post-completion ${event.fields?.kind ?? "effectful event"} was dropped at event ${index + 1}`,
        );
      }
      if (
        terminalOutput.has(name) &&
        !(ownsPostCompletionOutput && index > openIndex && index < closeIndex)
      ) {
        fail(
          `assistant output ${name} escaped terminal response at event ${index + 1}`,
        );
      }
    }
  }
}
