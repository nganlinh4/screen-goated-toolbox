import type { Item } from "./types";
import { canSubmitItem } from "./explicit-submission.ts";

export function claimNextQueued(items: Item[]): Item | undefined {
  const item = items.find((value) =>
    value.stage === "queued" && !value.submitted && !value.jobId && canSubmitItem(value)
  );
  if (item) item.submitted = true;
  return item;
}

export function releaseDispatchClaim(item: Item): void {
  if (!item.jobId) item.submitted = false;
}

export function advanceMissingStatusPoll(
  previous: number,
  limit: number,
): { count: number; timedOut: boolean } {
  const count = Math.max(0, previous) + 1;
  return { count, timedOut: count >= Math.max(1, limit) };
}
