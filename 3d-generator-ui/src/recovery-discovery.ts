/** Keeps local submissions from being rediscovered while recovery is pending. */
import type { JobStatus, QueueItem } from "./types";

export function matchingPublishedHistory(status: JobStatus, items: QueueItem[]): QueueItem | undefined {
  if (status.stage !== "done" || !status.outputPath) return undefined;
  const path = status.outputPath.toLowerCase();
  return items.find((item) => item.historyId && item.state === "done"
    && item.result?.outputPath?.toLowerCase() === path);
}

export class RecoveryDiscovery {
  private readonly seen = new Set<string>();
  private inFlight = 0;

  beginSubmission(): (jobId?: string | null) => void {
    this.inFlight += 1;
    let finished = false;
    return (jobId) => {
      if (finished) return;
      finished = true;
      this.inFlight -= 1;
      this.remember(jobId);
    };
  }

  canDiscover(): boolean { return this.inFlight === 0; }
  remember(jobId?: string | null): void { if (jobId) this.seen.add(jobId); }
  isUnseen(jobId: string): boolean { return !this.seen.has(jobId); }
}
