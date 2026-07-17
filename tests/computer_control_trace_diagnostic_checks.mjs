export function diagnosticCompletenessFailures(events) {
  const failures = [];

  const typedErrorsByAction = new Map();
  for (const event of events.filter((item) => item.event === "typed_error")) {
    if (event.action_id == null) continue;
    const count = typedErrorsByAction.get(event.action_id) ?? 0;
    typedErrorsByAction.set(event.action_id, count + 1);
  }
  const uncorrelatedActions = [];
  for (const event of events.filter((item) => item.event === "action_outcome")) {
    const fields = event.fields ?? {};
    const failed = fields.ok === false || fields.execution_ok === false;
    if (!failed || fields.cancelled === true) continue;
    const errorCount = typedErrorsByAction.get(event.action_id) ?? 0;
    if (errorCount !== 1) {
      uncorrelatedActions.push(`${event.action_id}(${errorCount})`);
    }
  }
  if (uncorrelatedActions.length > 0) {
    failures.push(
      `${uncorrelatedActions.length} failed action(s) lack exactly one correlated typed error: ${uncorrelatedActions.join(", ")}`,
    );
  }

  const responseGaps = [];
  const responseGapFields = new Set();
  for (const event of events.filter((item) => item.event === "tool_response_sent")) {
    const missing = missingPositiveIntegers(event, [
      "response_byte_count",
      "result_byte_count",
      "generation_index",
    ]);
    if (missing.length > 0) {
      responseGaps.push(event.action_id);
      missing.forEach((field) => responseGapFields.add(field));
    }
  }
  if (responseGaps.length > 0) {
    failures.push(
      `${responseGaps.length} tool response(s) lack size/generation attribution` +
        ` (actions ${compactIntegerRanges(responseGaps)}; fields ${[...responseGapFields].join(",")})`,
    );
  }
  const immediateGaps = [];
  for (const event of events.filter(
    (item) => item.event === "immediate_tool_response_sent",
  )) {
    const missing = missingPositiveIntegers(event, ["response_byte_count", "generation_index"]);
    if (missing.length > 0) immediateGaps.push(missing.join(","));
  }
  if (immediateGaps.length > 0) {
    failures.push(`${immediateGaps.length} immediate tool response(s) lack size/generation attribution`);
  }
  const usageGaps = [];
  for (const event of events.filter((item) => item.event === "model_usage")) {
    const missing = missingPositiveIntegers(event, ["usage_event_index", "generation_index"]);
    if (missing.length > 0) usageGaps.push(missing.join(","));
  }
  if (usageGaps.length > 0) {
    failures.push(`${usageGaps.length} model usage event(s) lack generation attribution`);
  }

  const transcriptGaps = [];
  for (const event of events.filter(
    (item) => item.event === "input_transcript_committed",
  )) {
    const missing = [];
    for (const field of ["source", "endpoint_reason", "finality"]) {
      if (typeof event.fields?.[field] !== "string") {
        missing.push(field);
      }
    }
    if (missing.length > 0) transcriptGaps.push(missing.join(","));
  }
  if (transcriptGaps.length > 0) {
    failures.push(`${transcriptGaps.length} committed transcript(s) lack endpoint/finality attribution`);
  }

  let researchGaps = 0;
  for (const event of events.filter((item) => item.event === "research_complete")) {
    if ((event.fields?.failure_count ?? 0) <= 0) continue;
    const codes = event.fields?.diagnostic_codes;
    if (!Array.isArray(codes) || codes.length === 0) {
      researchGaps += 1;
    }
  }
  if (researchGaps > 0) failures.push(`${researchGaps} failed research result(s) lack readable diagnostic_codes`);

  return failures;
}

function missingPositiveIntegers(event, fields) {
  return fields.filter((field) => {
    const value = event.fields?.[field];
    return !Number.isSafeInteger(value) || value <= 0;
  });
}

function compactIntegerRanges(values) {
  const sorted = [...new Set(values.filter(Number.isSafeInteger))].sort((a, b) => a - b);
  if (sorted.length === 0) return "unknown";
  const ranges = [];
  let start = sorted[0];
  let end = start;
  for (const value of sorted.slice(1)) {
    if (value === end + 1) {
      end = value;
      continue;
    }
    ranges.push(start === end ? `${start}` : `${start}-${end}`);
    start = value;
    end = value;
  }
  ranges.push(start === end ? `${start}` : `${start}-${end}`);
  return ranges.join(",");
}
