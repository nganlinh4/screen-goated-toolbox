import type { JobStatus } from "./types";

/**
 * A base result continues into part separation only when it was requested and
 * the user has not since cancelled the item; a job can publish its base model
 * just before a cancel reaches it.
 */
export function shouldStartAutomaticSegmentation(
  requested: boolean,
  status?: JobStatus,
  cancelRequested = false,
) {
  return Boolean(
    requested
      && !cancelRequested
      && status?.stage === "done"
      && status.jobId
      && status.canSegment
      && !status.isSegmented,
  );
}
