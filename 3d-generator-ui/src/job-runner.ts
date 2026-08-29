import { t } from "./i18n";
import type {
  AppState,
  JobStatus,
  QueueItem,
  RefineRequest,
  StartJobRequest,
} from "./types";
import type { GenerationMode } from "./generation-mode";
import {
  advanceMissingStatusPoll,
  dispatchAllSubmissions,
} from "./durable-dispatch";
import { frozenGenerationSettings } from "./recovery-settings";
import {
  canSubmitItem,
  freshSubmissionSession,
  needsFreshSubmissionSession,
} from "./submission-policy";
import { shouldStartAutomaticSegmentation } from "./automatic-segmentation";
import { retainPublishedDownload } from "./result-files";
import {
  AUTOMATIC_SEGMENTATION_START,
  GENERATION_WITH_SEGMENTATION_RANGE,
  automaticSegmentationRange,
  type ProgressRange,
} from "./progress-policy";

type NormalizedSettings = {
  mode: GenerationMode;
  polycount: number;
  autoSegment: boolean;
};

type JobRunnerOptions = {
  state: AppState;
  busyStages: Set<JobStatus["stage"]>;
  invoke: <T = unknown>(cmd: string, args?: unknown) => Promise<T>;
  normalizeSettings: (item: QueueItem) => NormalizedSettings;
  selectedItem: () => QueueItem | undefined;
  pathLeaf: (path: string) => string;
  displayItem: (item: QueueItem) => Promise<void>;
  refreshHistory: () => Promise<void>;
  updateUi: () => void;
  beginProgress: (item: QueueItem, estimateMs: number, range?: ProgressRange) => void;
};

const MAX_MISSING_STATUS_POLLS = 75;

function delay(milliseconds: number) {
  return new Promise((resolve) => window.setTimeout(resolve, milliseconds));
}

export class JobRunner {
  private monitorActive = false;
  private readonly missingStatusPolls = new Map<string, number>();

  constructor(private readonly options: JobRunnerOptions) {}

  submitSelected() {
    const { state, selectedItem, updateUi } = this.options;
    const item = selectedItem();
    if (!item || !canSubmitItem(item)) return;
    if (!needsFreshSubmissionSession(item)) {
      item.createdAtMs = Date.now();
      item.submitted = true;
      item.outputDir = state.outputDir;
      item.cancelRequested = false;
    } else {
      const submission = freshSubmissionSession(
        item,
        `submission_${crypto.randomUUID()}`,
        state.outputDir,
      );
      state.items.push(submission);
      state.selectedId = submission.id;
    }
    updateUi();
    void this.processQueue();
  }

  async processQueue() {
    const { state, updateUi } = this.options;
    if (state.queueActive) return;
    state.queueActive = true;
    state.cancelRequested = false;
    updateUi();
    await dispatchAllSubmissions(
      state.items,
      (item) => this.runItem(item),
      () => state.cancelRequested,
    );
    state.queueActive = false;
    state.cancelRequested = false;
    updateUi();
  }

  async segmentSelected() {
    const item = this.options.selectedItem();
    if (item) await this.startRefinement(item, {
      continuationId: item.result?.jobId || "",
      kind: "separate_parts",
      segmentationLevel: "detailed",
    });
  }

  async refineSelected(settings: Omit<RefineRequest, "continuationId">) {
    const item = this.options.selectedItem();
    if (!item?.result?.jobId) return;
    await this.startRefinement(item, { ...settings, continuationId: item.result.jobId });
  }

  private async startRefinement(
    parent: QueueItem,
    request: RefineRequest,
    continuedProgress?: number,
  ) {
    const {
      state, beginProgress, invoke, updateUi,
    } = this.options;
    if (
      !parent.result?.jobId
      || !parent.result.canRefine && !parent.result.canSegment
    ) return;
    const item: QueueItem = {
      ...parent,
      id: `revision_${Date.now()}_${crypto.randomUUID()}`,
      sourceProvenance: "presentation",
      submitted: true,
      state: "running",
      historyId: undefined,
      createdAtMs: Date.now(),
      exportedNames: undefined,
      result: {
        ...parent.result,
        stage: "refining",
        progressText: t("creating"),
        parentRevisionId: parent.result.jobId,
        canRefine: false,
        canSegment: false,
        supportedActions: [],
        availableActions: [],
      },
    };
    state.items.push(item);
    state.selectedId = item.id;
    state.runningIds.add(item.id);
    beginProgress(
      item,
      120_000,
      continuedProgress === undefined
        ? undefined
        : automaticSegmentationRange(continuedProgress),
    );
    updateUi();
    const baseResult = item.result!;
    parent.result.canRefine = false;
    parent.result.canSegment = false;
    parent.result.supportedActions = [];
    parent.result.availableActions = [];
    try {
      const initial = await invoke<JobStatus>("refine_model", request);
      if (!initial.jobId) throw new Error("The model job did not return an ID.");
      this.applyJobStatus(item, initial);
      if (this.options.busyStages.has(initial.stage)) {
        this.startJobMonitor();
      } else {
        await this.finishTrackedItem(item, initial);
      }
    } catch {
      item.state = "failed";
      item.result = {
        ...baseResult,
        stage: "failed",
        progressText: t("interrupted"),
        error: "interrupted",
      };
      state.runningIds.delete(item.id);
    } finally {
      updateUi();
    }
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
          status.jobId
          && status.sourceImagePath
          && frozenGenerationSettings(status)
          && (busyStages.has(status.stage) || status.stage === "done" && status.outputPath)
        ) {
          recoverable.set(status.jobId, status);
        }
      }
      const knownJobIds = new Set(
        state.items.map((item) => item.result?.jobId).filter(Boolean),
      );
      const items = [...recoverable.values()]
        .filter((status) => !knownJobIds.has(status.jobId))
        .map((status, index): QueueItem => {
        const path = status.sourceImagePath!;
        const name = pathLeaf(path);
        const running = busyStages.has(status.stage);
        const settings = frozenGenerationSettings(status)!;
        return {
          id: `recovered_${Date.now()}_${index}`,
          batchId: `recovered_batch_${Date.now()}_${index}`,
          path,
          sourceProvenance: "presentation",
          name,
          extension: name.split(".").pop()?.toUpperCase() || t("image"),
          generationMode: settings.generationMode,
          polycount: settings.polycount,
          autoSegment: settings.autoSegment,
          instruction: settings.instruction,
          outputDir: settings.outputDir,
          submitted: true,
          state: running ? "running" : "done",
          result: status,
          operationStartedAt:
            running ? Date.now() - Math.max(0, status.elapsedMs || 0) : undefined,
          estimatedTotalMs: status.estimatedTotalMs || 240_000,
          displayedProgress: status.progressRatio || 0,
          createdAtMs: Date.now() - Math.max(0, status.elapsedMs || 0),
        };
      });
      if (!items.length) {
        updateUi();
        return;
      }
      const latest = items[items.length - 1];
      state.items.push(...items);
      state.selectedId = latest.id;
      state.selectedStatus = latest.result!;
      for (const item of items) {
        if (item.state === "running") state.runningIds.add(item.id);
      }
      updateUi();
      await displayItem(latest);
      for (const item of items) {
        if (shouldStartAutomaticSegmentation(item.autoSegment, item.result)) {
          item.displayedProgress = AUTOMATIC_SEGMENTATION_START;
          item.progressContinues = true;
          await this.startRefinement(item, {
            continuationId: item.result?.jobId || "",
            kind: "separate_parts",
            segmentationLevel: "detailed",
          }, item.displayedProgress);
        }
      }
      this.startJobMonitor();
    } catch {
      updateUi();
    }
  }

  private applyJobStatus(item: QueueItem, status: JobStatus) {
    const { state, busyStages, displayItem, updateUi } = this.options;
    status = retainPublishedDownload(item.result, status);
    if (status.projectId) item.batchId = status.projectId;
    if (busyStages.has(status.stage)) {
      if (!item.operationStartedAt) {
        item.operationStartedAt = Date.now() - Math.max(0, status.elapsedMs || 0);
        item.displayedProgress = Math.max(0, status.progressRatio || 0);
      }
      if (status.estimatedTotalMs) item.estimatedTotalMs = status.estimatedTotalMs;
    }
    if (state.selectedId === item.id) state.selectedStatus = status;
    item.result = status;
    if (status.outputPath && state.selectedId === item.id) void displayItem(item);
    updateUi();
  }

  private async runItem(item: QueueItem) {
    const {
      state, normalizeSettings, beginProgress, displayItem, invoke, updateUi,
    } = this.options;
    const settings = normalizeSettings(item);
    state.runningIds.add(item.id);
    item.state = "running";
    beginProgress(
      item,
      240_000,
      settings.autoSegment ? GENERATION_WITH_SEGMENTATION_RANGE : undefined,
    );
    if (state.selectedId === item.id) await displayItem(item);
    if (item.cancelRequested) {
      item.state = "cancelled";
      state.runningIds.delete(item.id);
      updateUi();
      return;
    }
    const request: StartJobRequest = {
      imagePath: item.path,
      outputDir: item.outputDir || state.outputDir || null,
      polycount: settings.polycount,
      mode: "topology_mesh",
      generationMode: settings.mode,
      outputFormat: "glb_plain",
      autoSegment: settings.autoSegment,
      segmentationMode: settings.autoSegment ? "parts" : "none",
    };
    if (
      state.generationCapabilities.ready
      && state.generationCapabilities.optionalInstruction[settings.mode]
      && item.instruction?.trim()
    ) {
      request.instruction = item.instruction.trim();
    }
    try {
      const initial = await invoke<JobStatus>("start_job", request);
      if (!initial.jobId) throw new Error("The model job did not return an ID.");
      if (item.cancelRequested && initial.jobId) {
        const cancelled = await invoke<JobStatus>("cancel_job", { jobId: initial.jobId });
        await this.finishTrackedItem(item, cancelled);
        return;
      }
      this.applyJobStatus(item, initial);
      if (this.options.busyStages.has(initial.stage)) this.startJobMonitor();
      else await this.finishTrackedItem(item, initial);
    } catch {
      item.state = "failed";
      item.result = {
        stage: "failed",
        progressText: t("interrupted"),
        error: "interrupted",
        runtimeStatus: state.selectedStatus.runtimeStatus,
      };
      state.runningIds.delete(item.id);
      updateUi();
    }
  }

  private startJobMonitor() {
    if (this.monitorActive) return;
    this.monitorActive = true;
    void this.monitorJobs();
  }

  private async monitorJobs() {
    const { state, invoke, busyStages } = this.options;
    try {
      while (state.runningIds.size) {
        await delay(800);
        let statuses: JobStatus[];
        try {
          statuses = await invoke<JobStatus[]>("job_statuses");
        } catch {
          continue;
        }
        const byId = new Map(statuses.map((status) => [status.jobId, status]));
        for (const itemId of [...state.runningIds]) {
          const item = state.items.find((candidate) => candidate.id === itemId);
          if (!item) {
            state.runningIds.delete(itemId);
            this.missingStatusPolls.delete(itemId);
            continue;
          }
          if (!item.result?.jobId) {
            this.missingStatusPolls.delete(itemId);
            continue;
          }
          const status = byId.get(item.result.jobId);
          if (!status) {
            const missing = advanceMissingStatusPoll(
              this.missingStatusPolls.get(itemId) || 0,
              MAX_MISSING_STATUS_POLLS,
            );
            if (!missing.timedOut) {
              this.missingStatusPolls.set(itemId, missing.count);
              continue;
            }
            item.state = "failed";
            item.result = {
              ...item.result,
              stage: "failed",
              progressText: t("interrupted"),
              error: "interrupted",
            };
            state.runningIds.delete(itemId);
            this.missingStatusPolls.delete(itemId);
            this.options.updateUi();
            continue;
          }
          this.missingStatusPolls.delete(itemId);
          this.applyJobStatus(item, status);
          if (!busyStages.has(status.stage)) await this.finishTrackedItem(item, status);
        }
      }
    } finally {
      this.monitorActive = false;
      if (state.runningIds.size) this.startJobMonitor();
    }
  }

  private async finishTrackedItem(item: QueueItem, status: JobStatus) {
    const { state, displayItem, refreshHistory, updateUi } = this.options;
    status = retainPublishedDownload(item.result, status);
    const startAutomaticSegmentation = shouldStartAutomaticSegmentation(
      item.autoSegment,
      status,
    );
    item.result = status;
    item.state =
      status.stage === "done"
        ? "done"
        : status.stage === "cancelled"
          ? "cancelled"
          : "failed";
    state.runningIds.delete(item.id);
    this.missingStatusPolls.delete(item.id);
    if (startAutomaticSegmentation) {
      item.displayedProgress = Math.max(
        item.displayedProgress || 0,
        AUTOMATIC_SEGMENTATION_START,
      );
      item.progressContinues = true;
    }
    if (item.state === "done") {
      if (state.selectedId === item.id) await displayItem(item);
      await refreshHistory();
    }
    updateUi();
    if (startAutomaticSegmentation) {
      await this.startRefinement(item, {
        continuationId: status.jobId || "",
        kind: "separate_parts",
        segmentationLevel: "detailed",
      }, item.displayedProgress);
    }
  }
}
