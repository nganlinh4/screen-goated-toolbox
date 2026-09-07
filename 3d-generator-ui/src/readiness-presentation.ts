export function readinessLabel(
  busy: boolean,
  unavailable: boolean,
  preparationStatus: string,
): "working" | "unavailable" | "ready" | "preparing" {
  if (busy) return "working";
  if (unavailable) return "unavailable";
  return preparationStatus === "ready" ? "ready" : "preparing";
}

export function readinessIsError(busy: boolean, unavailable: boolean) {
  return !busy && unavailable;
}
