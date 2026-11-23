import { t } from "./i18n";
import { validateInput } from "./input-validation";
import { showInputDialog } from "./input-dialog";
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
import { matchingPublishedHistory, RecoveryDiscovery } from "./recovery-discovery";
import {
  canSubmitItem,
  freshSubmissionSession,
  needsFreshSubmissionSession,
} from "./submission-policy";
import { shouldStartAutomaticSegmentation } from "./automatic-segmentation";
import { cancellationOutcome } from "./cancellation-outcome";
import { claimContinuation } from "./refinement-policy";
import { retainPublishedDownload } from "./result-files";
import {
  AUTOMATIC_SEGMENTATION_START,
  GENERATION_WITH_SEGMENTATION_RANGE,
  automaticSegmentationRange,
  pendingRevisionStatus,
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
  private readonly recoveryDiscovery = new RecoveryDiscovery();

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
      () => state.items,
      (item) => this.runItem(item),
      () => state.cancelRequested,
    );
    state.queueActive = false;
    state.cancelRequested = false;
    updateUi();
  }

  async cancelItem(item: QueueItem) {
    const { state, updateUi } = this.options;
    item.cancelRequested = true;
    const jobId = item.result?.jobId;
    if (!jobId) {
      // runItem cancels a job whose start is still in flight once it has an ID.
      item.state = "cancelled";
      state.runningIds.delete(item.id);
      updateUi();
      return;
    }
    await this.cancelTrackedJob(item, jobId);
  }

  /**
   * Settles an item only from a confirmed reply for its own job; otherwise the
   * job is still running (or unconfirmed) and stays tracked until its real
   * outcome arrives.
   */
  private async cancelTrackedJob(item: QueueItem, jobId: string) {
    const { state, invoke, busyStages, updateUi } = this.options;
    const reply = await invoke<JobStatus>("cancel_job", { jobId }).catch(() => undefined);
    const outcome = cancellationOutcome(jobId, reply, busyStages);
    if (outcome.kind === "settled") {
      await this.finishTrackedItem(item, outcome.status);
      return;
    }
    if (outcome.kind === "running") this.applyJobStatus(item, outcome.status);
    if (item.state === "running") state.runningIds.add(item.id);
    this.startJobMonitor();
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
      cancelRequested: false,
      state: "running",
      historyId: undefined,
      createdAtMs: Date.now(),
      exportedNames: undefined,
      retainedModelPath: parent.loadedModelPath || parent.result.outputPath || undefined,
      result: pendingRevisionStatus(parent.result, t("creating")),
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
    const releaseContinuation = claimContinuation(parent.result);
    let admitted = false;
    const finishSubmission = this.recoveryDiscovery.beginSubmission();
    try {
      const initial = await invoke<JobStatus>("refine_model", request);
      admitted = true;
      finishSubmission(initial.jobId);
      if (!initial.jobId) throw new Error("The model job did not return an ID.");
      this.applyJobStatus(item, initial);
      if (this.options.busyStages.has(initial.stage)) {
        this.startJobMonitor();
      } else {
        await this.finishTrackedItem(item, initial);
      }
    } catch {
      if (!admitted) releaseContinuation();
      item.state = "failed";
      item.result = {
        ...baseResult,
        stage: "failed",
        progressText: t("interrupted"),
        error: "interrupted",
      };
      state.runningIds.delete(item.id);
    } finally {
      finishSubmission();
      updateUi();
    }
  }

  async restoreCurrentJobs(): Promise<boolean> {
    const {
      state, invoke, busyStages, pathLeaf, updateUi, displayItem,
    } = this.options;
    if (!this.recoveryDiscovery.canDiscover()) return false;
    try {
      const statuses = await invoke<JobStatus[]>("job_statuses");
      if (!this.recoveryDiscovery.canDiscover()) return false;
      const recoverable = new Map<string, JobStatus>();
      for (const status of statuses) {
        if (
          status.jobId
          && (status.error === "recovery_deadline" || (
            status.sourceImagePath
            && frozenGenerationSettings(status)
            && (busyStages.has(status.stage) || status.stage === "failed"
              || status.stage === "done" && status.outputPath)
          ))
        ) {
          recoverable.set(status.jobId, status);
        }
      }
      const knownJobIds = new Set(
        state.items.map((item) => item.result?.jobId).filter(Boolean),
      );
      for (const id of knownJobIds) this.recoveryDiscovery.remember(id);
      const matchedHistory: QueueItem[] = [];
      for (const status of recoverable.values()) {
        if (!this.recoveryDiscovery.isUnseen(status.jobId!)) continue;
        const existing = matchingPublishedHistory(status, state.items);
        if (!existing) continue;
        const settings = frozenGenerationSettings(status)!;
        existing.generationMode = settings.generationMode;
        existing.polycount = settings.polycount;
        existing.autoSegment = settings.autoSegment;
        existing.segmentationLevel = settings.segmentationLevel;
        existing.topology = settings.topology;
        existing.instruction = settings.instruction;
        existing.outputDir = settings.outputDir;
        this.applyJobStatus(existing, status);
        matchedHistory.push(existing);
      }
      const items = [...recoverable.values()]
        .filter((status) => this.recoveryDiscovery.isUnseen(status.jobId!))
        .map((status, index): QueueItem => {
        const path = status.sourceImagePath || "";
        const name = path ? pathLeaf(path) : t("interrupted");
        const running = busyStages.has(status.stage);
        const settings = frozenGenerationSettings(status) || {
          generationMode: "quality" as const,
          polycount: 5_000,
          autoSegment: false,
          segmentationLevel: "detailed" as const,
          topology: undefined,
          instruction: undefined,
          outputDir: state.outputDir,
        };
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
          segmentationLevel: status.segmentationLevel || "detailed",
          topology: settings.topology,
          instruction: settings.instruction,
          outputDir: settings.outputDir,
          submitted: true,
          state: running ? "running" : status.stage === "done" ? "done" : "failed",
          result: status,
          operationStartedAt:
            running ? Date.now() - Math.max(0, status.elapsedMs || 0) : undefined,
          estimatedTotalMs: status.estimatedTotalMs || 240_000,
          displayedProgress: status.progressRatio || 0,
          createdAtMs: Date.now() - Math.max(0, status.elapsedMs || 0),
        };
      });
      if (!items.length && !matchedHistory.length) {
        updateUi();
        return true;
      }
      const latest = items[items.length - 1];
      const selectLatest = Boolean(latest) && !state.items.some((item) => item.id === state.selectedId);
      state.items.push(...items);
      for (const item of items) this.recoveryDiscovery.remember(item.result!.jobId);
      if (selectLatest) {
        state.selectedId = latest.id;
        state.selectedStatus = latest.result!;
      }
      for (const item of [...items, ...matchedHistory]) {
        if (item.state === "running") state.runningIds.add(item.id);
      }
      updateUi();
      if (selectLatest) await displayItem(latest);
      for (const item of [...items, ...matchedHistory]) {
        if (shouldStartAutomaticSegmentation(item.autoSegment, item.result, item.cancelRequested)) {
          item.displayedProgress = AUTOMATIC_SEGMENTATION_START;
          item.progressContinues = true;
          await this.startRefinement(item, {
            continuationId: item.result?.jobId || "",
            kind: "separate_parts",
            segmentationLevel: item.segmentationLevel,
          }, item.displayedProgress);
        }
      }
      this.startJobMonitor();
      return true;
    } catch {
      updateUi();
      return false;
    }
  }

  private applyJobStatus(item: QueueItem, status: JobStatus) {
    const { state, busyStages, displayItem, updateUi } = this.options;
    this.recoveryDiscovery.remember(status.jobId);
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
    while (true) {
      const inputError = await validateInput(invoke, item.path, settings.mode);
      if (item.cancelRequested || state.cancelRequested) {
        item.state = "cancelled";
        updateUi();
        return;
      }
      if (!inputError) break;
      item.state = "failed";
      item.result = { stage: "failed", progressText: "", error: inputError,
        runtimeStatus: state.selectedStatus.runtimeStatus };
      updateUi();
      const action = await showInputDialog(inputError, item.name);
      if (action === "retry") continue;
      if (action === "choose") document.getElementById("addImagesButton")?.click();
      return;
    }
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
      topology: item.topology,
      generationMode: settings.mode,
      outputFormat: "glb_plain",
      autoSegment: settings.autoSegment,
      segmentationLevel: item.segmentationLevel,
      segmentationMode: settings.autoSegment ? "parts" : "none",
    };
    if (
      state.generationCapabilities.ready
      && state.generationCapabilities.optionalInstruction[settings.mode]
      && item.instruction?.trim()
    ) {
      request.instruction = item.instruction.trim();
    }
    const finishSubmission = this.recoveryDiscovery.beginSubmission();
    try {
      const initial = await invoke<JobStatus>("start_job", request);
      finishSubmission(initial.jobId);
      if (!initial.jobId) throw new Error("The model job did not return an ID.");
      if (item.cancelRequested) {
        // The cancel arrived while start_job was in flight; the job exists now.
        item.state = "running";
        this.applyJobStatus(item, initial);
        await this.cancelTrackedJob(item, initial.jobId);
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
    } finally {
      finishSubmission();
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
      item.cancelRequested,
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
        segmentationLevel: item.segmentationLevel,
      }, item.displayedProgress);
    }
  }
}
