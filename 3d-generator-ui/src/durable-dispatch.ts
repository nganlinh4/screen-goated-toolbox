import type { QueueItem } from "./types";
import { canSubmitItem } from "./submission-policy.ts";

export function claimNextSubmission(items: QueueItem[]): QueueItem | undefined {
  const item = items.find((candidate) =>
    candidate.state === "queued"
      && candidate.submitted
      && !candidate.result?.jobId
      && canSubmitItem(candidate)
  );
  if (item) item.state = "running";
  return item;
}

export async function dispatchAllSubmissions(
  items: QueueItem[],
  dispatch: (item: QueueItem) => Promise<void>,
  shouldStop: () => boolean = () => false,
): Promise<void> {
  while (!shouldStop()) {
    const item = claimNextSubmission(items);
    if (!item) return;
    await dispatch(item);
  }
}

export function advanceMissingStatusPoll(
  previous: number,
  limit: number,
): { count: number; timedOut: boolean } {
  const count = Math.max(0, previous) + 1;
  return { count, timedOut: count >= Math.max(1, limit) };
}
