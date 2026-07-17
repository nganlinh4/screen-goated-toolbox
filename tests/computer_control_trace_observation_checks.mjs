function stableJson(value) {
  if (Array.isArray(value)) return value.map(stableJson);
  if (!value || typeof value !== "object") return value;
  return Object.fromEntries(
    Object.keys(value)
      .sort()
      .map((key) => [key, stableJson(value[key])]),
  );
}

function stableSurfaceKey(surface) {
  if (!surface || typeof surface !== "object") return "none";
  if (surface.kind === "browser") {
    return `browser:${surface.tab_id ?? "?"}:${JSON.stringify(surface.document_id ?? null)}`;
  }
  if (surface.kind === "native") {
    return `native:${surface.hwnd ?? "?"}:${surface.pid ?? "?"}:${surface.generation ?? "?"}`;
  }
  return JSON.stringify(stableJson(surface));
}

export function observationChurnFailures(events, maxRepeatedReads = 5) {
  const requestByAction = new Map(
    events
      .filter((event) => event.event === "tool_call_payload" && event.action_id != null)
      .map((event) => [
        event.action_id,
        JSON.stringify(stableJson(event.fields?.args ?? null)),
      ]),
  );
  const failures = [];
  let run = null;
  for (const event of events.filter((item) => item.event === "action_outcome")) {
    const observational =
      event.fields?.execution_ok === true &&
      event.fields?.postcondition?.effect === "observation_or_query";
    if (!observational) {
      run = null;
      continue;
    }
    const request = requestByAction.get(event.action_id) ?? "request-unavailable";
    const key =
      `${event.fields?.effective_tool ?? "unknown"}|` +
      `${stableSurfaceKey(event.fields?.post_surface)}|${request}`;
    if (run?.key === key) {
      run.count += 1;
    } else {
      run = { key, count: 1, firstAction: event.action_id };
    }
    if (run.count > maxRepeatedReads) {
      failures.push(
        `actions ${run.firstAction}-${event.action_id} repeat the same successful observation ` +
          `${run.count} times against one unchanged target; expected a route change or completion`,
      );
      run = null;
    }
  }
  return failures;
}
