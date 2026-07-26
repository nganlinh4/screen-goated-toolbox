import { t } from "./i18n";
import type {
  AppState,
  JobStatus,
  QueueItem,
  StartJobRequest,
} from "./types";
import type { GenerationMode } from "./generation-mode";

type NormalizedSettings = {
  mode: GenerationMode;
  polycount: number;
  autoSegment: boolean;
};

type JobRunnerOptions = {
  state: AppState;
  busyStages: Set<JobStatus["stage"]>;
  maxParallelJobs: number;
  invoke: <T = unknown>(cmd: string, args?: unknown) => Promise<T>;
  normalizeSettings: (item: QueueItem) => NormalizedSettings;
  selectedItem: () => QueueItem | undefined;
  pendingItems: () => QueueItem[];
  activeJobCount: () => number;
  pathLeaf: (path: string) => string;
  displayItem: (item: QueueItem) => Promise<void>;
  loadDepthFor: (item: QueueItem, path: string) => Promise<void>;
  refreshHistory: () => Promise<void>;
  updateUi: () => void;
  beginProgress: (item: QueueItem, estimateMs: number) => void;
};

function delay(milliseconds: number) {
  return new Promise((resolve) => window.setTimeout(resolve, milliseconds));
}

export class JobRunner {
  constructor(private readonly options: JobRunnerOptions) {}

  submitSelected() {
    const { state, selectedItem, updateUi } = this.options;
    const item = selectedItem();
    if (!item) return;
    if (item.state === "queued" && !item.submitted) {
      item.submitted = true;
    } else if (item.state === "done" || item.state === "failed" || item.state === "cancelled") {
      item.state = "queued";
      item.submitted = true;
      item.historyId = undefined;
      item.createdAtMs = undefined;
      item.result = undefined;
      item.modelStats = undefined;
    }
    updateUi();
    if (!state.queueActive) void this.processQueue();
  }

  async processQueue() {
    const {
      state, pendingItems, activeJobCount, maxParallelJobs, updateUi,
    } = this.options;
    if (state.queueActive) return;
    state.queueActive = true;
    state.cancelRequested = false;
    updateUi();
    const active = new Map<string, Promise<void>>();
    while (!state.cancelRequested) {
      while (activeJobCount() < maxParallelJobs) {
        const next = pendingItems()[0];
        if (!next) break;
        const operation = this.runItem(next).finally(() => active.delete(next.id));
        active.set(next.id, operation);
      }
      if (!active.size) {
        if (pendingItems().length && activeJobCount() >= maxParallelJobs) {
          await delay(400);
          continue;
        }
        break;
      }
      await Promise.race(active.values());
    }
    await Promise.allSettled(active.values());
    state.queueActive = false;
    state.cancelRequested = false;
    updateUi();
  }

  async segmentSelected() {
    const {
      state, selectedItem, activeJobCount, maxParallelJobs, beginProgress,
      invoke, displayItem, refreshHistory, pendingItems, updateUi,
    } = this.options;
    const item = selectedItem();
    if (
      !item?.result?.jobId
      || item.result.isSegmented
      || !item.result.canSegment
      || activeJobCount() >= maxParallelJobs
    ) return;
    const continuationId = item.result.jobId;
    state.runningIds.add(item.id);
    item.state = "running";
    beginProgress(item, 120_000);
    updateUi();
    try {
      const initial = await invoke<JobStatus>("segment_model", { continuationId });
      const final = await this.waitForJob(item, initial);
      item.result = final;
      item.state = final.stage === "done" ? "done" : "failed";
      if (item.state === "done") {
        await displayItem(item);
        await refreshHistory();
      }
    } catch (error) {
      item.state = "failed";
      item.result = {
        stage: "failed",
        progressText: String(error),
        error: String(error),
      };
    } finally {
      state.runningIds.delete(item.id);
      updateUi();
      this.startPreparationPolling();
      if (pendingItems().length && !state.queueActive) void this.processQueue();
    }
  }

  startPreparationPolling() {
    const { state, invoke, updateUi } = this.options;
    window.clearTimeout(state.preparationTimer);
    const token = ++state.preparationPollToken;
    const check = async () => {
      try {
        state.preparationStatus = await invoke<string>("runtime_preparation_status");
      } catch {
        state.preparationStatus = "not_ready";
      }
      if (token !== state.preparationPollToken) return;
      updateUi();
      const delayMs =
        state.preparationStatus === "preparing" || state.preparationStatus === "not_ready"
          ? 1_000
          : 15_000;
      state.preparationTimer = window.setTimeout(check, delayMs);
    };
    void check();
  }

  async restoreCurrentJobs() {
    const {
      state, invoke, busyStages, pathLeaf, updateUi, displayItem,
    } = this.options;
    try {
      const statuses = await invoke<JobStatus[]>("job_statuses");
      const recoverable = new Map<string, JobStatus>();
      for (const status of statuses) {
        if (
          status.sourceImagePath
          && (busyStages.has(status.stage) || status.stage === "done" && status.outputPath)
        ) {
          recoverable.set(status.sourceImagePath, status);
        }
      }
      const items = [...recoverable.values()].map((status, index): QueueItem => {
        const path = status.sourceImagePath!;
        const name = pathLeaf(path);
        const running = busyStages.has(status.stage);
        return {
          id: `recovered_${Date.now()}_${index}`,
          batchId: `recovered_batch_${Date.now()}_${index}`,
          path,
          name,
          extension: name.split(".").pop()?.toUpperCase() || t("image"),
          generationMode: status.generationMode || "quality",
          polycount: 5_000,
          autoSegment: Boolean(status.isSegmented),
          submitted: true,
          state: running ? "running" : "done",
          result: status,
          operationStartedAt:
            running ? Date.now() - Math.max(0, status.elapsedMs || 0) : undefined,
          estimatedTotalMs: status.estimatedTotalMs || 240_000,
          displayedProgress: status.progressRatio || 0,
        };
      });
      if (!items.length) {
        updateUi();
        return;
      }
      const latest = items[items.length - 1];
      state.items.push(...items);
      state.selectedId = latest.id;
      state.backendStatus = latest.result!;
      for (const item of items) {
        if (item.state === "running") state.runningIds.add(item.id);
      }
      updateUi();
      await displayItem(latest);
      await Promise.all(items
        .filter((item) => item.state === "running")
        .map(async (item) => {
          try {
            const final = await this.waitForJob(item, item.result!);
            item.result = final;
            item.state =
              final.stage === "done"
                ? "done"
                : final.stage === "cancelled"
                  ? "cancelled"
                  : "failed";
            if (state.selectedId === item.id && item.state === "done") await displayItem(item);
          } catch (error) {
            item.state = "failed";
            item.result = {
              stage: "failed",
              progressText: String(error),
              error: String(error),
            };
          } finally {
            state.runningIds.delete(item.id);
            updateUi();
          }
        }));
    } catch {
      updateUi();
    }
  }

  private applyBackendStatus(item: QueueItem, status: JobStatus) {
    const { state, busyStages, loadDepthFor, displayItem, updateUi } = this.options;
    if (busyStages.has(status.stage)) {
      if (!item.operationStartedAt) {
        item.operationStartedAt = Date.now() - Math.max(0, status.elapsedMs || 0);
        item.displayedProgress = Math.max(0, status.progressRatio || 0);
      }
      if (status.estimatedTotalMs) item.estimatedTotalMs = status.estimatedTotalMs;
    }
    if (state.selectedId === item.id) state.backendStatus = status;
    item.result = status;
    if (status.previewPath) void loadDepthFor(item, status.previewPath);
    if (status.outputPath && state.selectedId === item.id) void displayItem(item);
    updateUi();
  }

  private async waitForJob(item: QueueItem, initial: JobStatus) {
    const { busyStages, invoke } = this.options;
    let status = initial;
    this.applyBackendStatus(item, status);
    const jobId = status.jobId;
    if (!jobId) throw new Error("The model job did not return an ID.");
    while (busyStages.has(status.stage)) {
      await delay(800);
      status = await invoke<JobStatus>("job_status", { jobId });
      this.applyBackendStatus(item, status);
    }
    return status;
  }

  private async runItem(item: QueueItem) {
    const {
      state, normalizeSettings, beginProgress, displayItem, invoke,
      refreshHistory, updateUi,
    } = this.options;
    const settings = normalizeSettings(item);
    state.runningIds.add(item.id);
    item.state = "running";
    beginProgress(item, 240_000);
    if (state.selectedId === item.id) await displayItem(item);
    const request: StartJobRequest = {
      imagePath: item.path,
      outputDir: state.outputDir || null,
      polycount: settings.polycount,
      mode: "topology_mesh",
      generationMode: settings.mode,
      outputFormat: "glb_plain",
      autoSegment: settings.autoSegment,
      segmentationMode: settings.autoSegment ? "parts" : "none",
    };
    try {
      const initial = await invoke<JobStatus>("start_job", request);
      const final = await this.waitForJob(item, initial);
      item.result = final;
      if (final.stage === "done") {
        item.state = "done";
        if (state.selectedId === item.id) await displayItem(item);
        await refreshHistory();
      } else if (final.stage === "cancelled") item.state = "cancelled";
      else item.state = "failed";
    } catch (error) {
      item.state = "failed";
      item.result = {
        stage: "failed",
        progressText: String(error),
        error: String(error),
        runtimeStatus: state.backendStatus.runtimeStatus,
      };
    } finally {
      state.runningIds.delete(item.id);
      updateUi();
      this.startPreparationPolling();
    }
  }
}
