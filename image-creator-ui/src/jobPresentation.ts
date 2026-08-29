import type { Copy } from "./i18n";
import { jobReferences, type JobStatus } from "./models";

export interface ProgressSnapshot {
  visible: boolean;
  percent: number;
  eta: string;
}

interface TrackedProgress {
  startedAt: number;
  estimatedTotalMs: number;
  displayed: number;
}

const PUBLIC_STAGES = [
  "queued",
  "preparing",
  "uploading",
  "generating",
  "finalizing",
  "done",
  "failed",
  "cancelled",
];

export function publicImageStage(stage: string): string {
  return PUBLIC_STAGES.includes(stage) ? stage : "preparing";
}

export function imageStatusLabel(job: JobStatus, copy: Copy): string {
  switch (publicImageStage(job.stage)) {
    case "queued": return copy.queued;
    case "preparing": return copy.preparing;
    case "uploading": return jobReferences(job).length ? copy.uploading : copy.preparing;
    case "generating": return copy.generating;
    case "finalizing": return copy.finalizing;
    case "done": return copy.ready;
    case "failed": return copy.failed;
    case "cancelled": return copy.cancelled;
    default: return copy.preparing;
  }
}

function isBusy(stage: string): boolean {
  return !["done", "failed", "cancelled", "draft"].includes(stage);
}

function remainingLabel(milliseconds: number, copy: Copy): string {
  if (milliseconds <= 15_000) return copy.almostThere;
  if (milliseconds < 60_000) return copy.lessMinute;
  return copy.aboutMinutes(Math.max(1, Math.ceil(milliseconds / 60_000)));
}

export class ImageProgressPresenter {
  private readonly tracked = new Map<string, TrackedProgress>();

  retain(jobs: JobStatus[]) {
    const activeIds = new Set(jobs.map((job) => job.jobId));
    for (const jobId of this.tracked.keys()) {
      if (!activeIds.has(jobId)) this.tracked.delete(jobId);
    }
  }

  snapshot(job: JobStatus | undefined, copy: Copy, now = Date.now()): ProgressSnapshot {
    if (!job || !isBusy(job.stage)) {
      return { visible: false, percent: job?.stage === "done" ? 100 : 0, eta: "" };
    }
    const elapsedFromRuntime = Math.max(0, job.elapsedMs || 0);
    const observedStart = now - elapsedFromRuntime;
    const existing = this.tracked.get(job.jobId);
    const tracked = existing || {
      startedAt: observedStart,
      estimatedTotalMs: Math.max(10_000, job.estimatedTotalMs || 180_000),
      displayed: 0,
    };
    tracked.startedAt = Math.min(tracked.startedAt, observedStart);
    if (job.estimatedTotalMs) {
      tracked.estimatedTotalMs = Math.max(10_000, job.estimatedTotalMs);
    }
    const elapsedMs = Math.max(elapsedFromRuntime, now - tracked.startedAt);
    const curved = Math.min(
      0.94,
      0.9 * (1 - Math.exp((-3 * elapsedMs) / tracked.estimatedTotalMs)),
    );
    const reported = Math.max(0, Math.min(0.94, job.progressRatio || 0));
    tracked.displayed = Math.max(tracked.displayed, curved, reported);
    this.tracked.set(job.jobId, tracked);
    return {
      visible: true,
      percent: Math.round(tracked.displayed * 100),
      eta: elapsedMs >= tracked.estimatedTotalMs
        ? copy.takingLonger
        : remainingLabel(tracked.estimatedTotalMs - elapsedMs, copy),
    };
  }

  sync(root: ParentNode, job: JobStatus | undefined, copy: Copy) {
    const state = this.snapshot(job, copy);
    const progress = root.querySelector<HTMLElement>("[data-job-progress]");
    const fill = progress?.querySelector<HTMLElement>("b");
    const eta = root.querySelector<HTMLElement>("[data-job-progress-eta]");
    progress?.classList.toggle("visible", state.visible);
    progress?.setAttribute("aria-valuenow", String(state.percent));
    if (fill) fill.style.width = `${state.percent}%`;
    eta?.classList.toggle("visible", state.visible);
    if (eta) eta.textContent = state.eta;
  }
}
