function effectClass(fields = {}) {
  if (fields.effect_verified === true || fields.effect_status === "verified") {
    return "verified";
  }
  if (fields.execution_ok === false || fields.ok === false) return "failed";
  if (
    fields.effect_may_have_occurred === true ||
    fields.effect_status === "may_have_occurred"
  ) {
    return "ambiguous";
  }
  if (fields.executed === false) return "no-effect";
  return typeof fields.effect_status === "string"
    ? fields.effect_status
    : "unknown";
}

// Outcomes that settle what state the turn left behind. Proven no-effect and
// read-only outcomes neither prove nor undo earlier progress.
const TERMINAL_EFFECT_CLASSES = new Set(["verified", "failed", "ambiguous"]);

export function completionEvidenceFailures(events) {
  const failures = [];
  const indexed = events.map((event, index) => ({ event, index }));
  const doneSummaries = indexed.filter(
    ({ event }) =>
      event.event === "turn_summary" && event.fields?.outcome === "done",
  );

  for (const completion of doneSummaries) {
    const turnId = completion.event.turn_id;
    const priorClasses = indexed
      .filter(
        ({ event, index }) =>
          index < completion.index &&
          event.turn_id === turnId &&
          event.event === "action_outcome" &&
          event.fields?.requested_tool !== "done",
      )
      .map(({ event }) => effectClass(event.fields));
    const latestTerminal = priorClasses.findLast((status) =>
      TERMINAL_EFFECT_CLASSES.has(status),
    );
    if (latestTerminal !== "verified") {
      const statuses = priorClasses.join(", ");
      failures.push(
        `turn ${turnId} accepted explicit done without a verified receipt for its latest effect` +
          (statuses ? ` (prior effects: ${statuses})` : ""),
      );
    }
  }
  return failures;
}
