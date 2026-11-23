import type { JobStatus } from "./types";

export function declaredRefinements(
  supportedActions: string[] | undefined,
  availableActions: string[] | undefined,
) {
  return new Set(supportedActions ?? availableActions ?? []);
}

export function hasDeclaredRefinements(
  supportedActions: string[] | undefined,
  availableActions: string[] | undefined,
) {
  return declaredRefinements(supportedActions, availableActions).size > 0;
}

type ContinuationFields = Pick<
  JobStatus,
  "canRefine" | "canSegment" | "supportedActions" | "availableActions"
>;

/**
 * Withdraws a parent's continuation while a refinement request is in flight.
 * The host only reserves the continuation once it admits the request, so the
 * returned function hands it back when the request is rejected (for example
 * a full queue) and the same model can be refined again.
 */
export function claimContinuation(result: ContinuationFields): () => void {
  const held: ContinuationFields = {
    canRefine: result.canRefine,
    canSegment: result.canSegment,
    supportedActions: result.supportedActions,
    availableActions: result.availableActions,
  };
  result.canRefine = false;
  result.canSegment = false;
  result.supportedActions = [];
  result.availableActions = [];
  return () => {
    Object.assign(result, held);
  };
}
