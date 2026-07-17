export function completionEvidenceFailures(events) {
  const failures = [];
  const indexed = events.map((event, index) => ({ event, index }));
  const doneSummaries = indexed.filter(
    ({ event }) =>
      event.event === "turn_summary" && event.fields?.outcome === "done",
  );

  for (const completion of doneSummaries) {
    const turnId = completion.event.turn_id;
    const priorOutcomes = indexed.filter(
      ({ event, index }) =>
        index < completion.index &&
        event.turn_id === turnId &&
        event.event === "action_outcome" &&
        event.fields?.requested_tool !== "done",
    );
    const verified = priorOutcomes.some(
      ({ event }) =>
        event.fields?.effect_verified === true ||
        event.fields?.effect_status === "verified",
    );
    if (!verified) {
      const statuses = priorOutcomes
        .map(({ event }) => {
          if (event.fields?.effect_verified === true) return "verified";
          if (event.fields?.execution_ok === false || event.fields?.ok === false) {
            return "failed";
          }
          if (event.fields?.effect_may_have_occurred === true) return "ambiguous";
          if (event.fields?.executed === false) return "no-effect";
          const status = event.fields?.effect_status;
          return typeof status === "string" ? status : "unknown";
        })
        .join(", ");
      failures.push(
        `turn ${turnId} accepted explicit done without a verified action receipt` +
          (statuses ? ` (prior effects: ${statuses})` : ""),
      );
    }
  }
  return failures;
}
