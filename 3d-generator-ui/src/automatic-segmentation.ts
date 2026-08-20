import type { JobStatus } from "./types";

export function shouldStartAutomaticSegmentation(
  requested: boolean,
  status?: JobStatus,
) {
  return Boolean(
    requested
      && status?.stage === "done"
      && status.jobId
      && status.canSegment
      && !status.isSegmented,
  );
}
