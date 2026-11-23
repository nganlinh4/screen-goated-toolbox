import type { JobStatus } from "./types";

export type CancellationOutcome =
  /** The job reached a terminal stage: cancelled, or done/failed first. */
  | { kind: "settled"; status: JobStatus }
  /** The host could not persist the cancel; the job keeps running. */
  | { kind: "running"; status: JobStatus }
  /** The reply does not describe this job, so nothing is known about it. */
  | { kind: "unconfirmed" };

/**
 * Classifies the host's cancel_job reply. The host returns the job's current
 * status, which is only "cancelled" when the cancellation was persisted, and it
 * falls back to its latest status of any job when the id is unknown.
 */
export function cancellationOutcome(
  jobId: string,
  reply: JobStatus | undefined,
  busyStages: ReadonlySet<string>,
): CancellationOutcome {
  if (!reply || reply.jobId !== jobId) return { kind: "unconfirmed" };
  return busyStages.has(reply.stage)
    ? { kind: "running", status: reply }
    : { kind: "settled", status: reply };
}
