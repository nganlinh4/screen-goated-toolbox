import type { Item, JobStatus } from "./types";

export function adoptRecoveredJobs(
  items: Item[],
  statuses: JobStatus[],
  basename: (path: string) => string,
): number {
  let added = 0;
  for (const status of statuses) {
    if (!["queued", "preparing", "generating", "finalizing", "cancelling", "failed"].includes(status.stage)
      || items.some((item) => item.jobId === status.jobId)) continue;
    const started = Date.now() - Math.max(0, status.elapsedMs || 0);
    items.push({
      id: `recovered_${status.jobId}`, batchId: `recovered_${status.jobId}`,
      path: status.sourceImagePath, sourceProvenance: "presentation",
      name: basename(status.sourceImagePath), model: status.model,
      backgroundMode: status.backgroundMode, outputDir: status.outputDir,
      stage: status.stage, submitted: true, jobId: status.jobId,
      progress: status.progressRatio, progressText: status.progressText,
      progressKey: status.progressKey, phase: status.phase, error: status.error,
      outputPath: status.outputPath, outputName: status.outputName,
      operationStartedAt: started, estimatedTotalMs: status.estimatedTotalMs,
      createdAtMs: started,
    });
    added += 1;
  }
  return added;
}
