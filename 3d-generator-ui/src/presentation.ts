import { generationSettings } from "./generation-mode";
import { locale, t, type MessageKey } from "./i18n";
import { ICONS } from "./layout";
import type { AppNodes, AppState, QueueItem, Stage } from "./types";
import type { ModelViewer, ModelStats, ShadingMode } from "./viewer";

type PresentationOptions = {
  state: AppState;
  nodes: AppNodes;
  viewer: ModelViewer;
  selectedItem: () => QueueItem | undefined;
  batchItems: (batchId: string) => QueueItem[];
  activeJobCount: () => number;
  isConfigurable: (item?: QueueItem) => boolean;
  isDraft: (item?: QueueItem) => boolean;
  normalizeSettings: (item: QueueItem) => ReturnType<typeof generationSettings>;
  maxParallelJobs: number;
  renderQueue: () => void;
  stripExtension: (name: string) => string;
};

export class ModelPresentation {
  constructor(private readonly options: PresentationOptions) {}

  applyTranslations() {
    document.querySelectorAll<HTMLElement>("[data-i18n]").forEach((node) => {
      node.textContent = t(node.dataset.i18n as MessageKey);
    });
    document.querySelectorAll<HTMLElement>("[data-i18n-title]").forEach((node) => {
      const value = t(node.dataset.i18nTitle as MessageKey);
      node.title = value;
      node.setAttribute("aria-label", value);
    });
    document.querySelectorAll<HTMLElement>("[data-i18n-aria]").forEach((node) => {
      node.setAttribute("aria-label", t(node.dataset.i18nAria as MessageKey));
    });
    const { state, nodes, stripExtension } = this.options;
    const referenceItem = state.items.find((item) => item.id === state.referencePreviewItemId);
    if (referenceItem) {
      nodes.referencePreviewImage.alt = t("referenceImageAlt", {
        name: stripExtension(referenceItem.name),
      });
    }
  }

  beginProgress(item: QueueItem, estimateMs: number) {
    item.operationStartedAt = Date.now();
    item.estimatedTotalMs = estimateMs;
    item.displayedProgress = 0;
  }

  updateProgressUi() {
    const { nodes, selectedItem } = this.options;
    const item = selectedItem();
    const busy = item?.state === "running";
    nodes.progressTrack.classList.toggle("visible", busy);
    nodes.statusEta.classList.toggle("visible", busy);
    if (!busy) {
      nodes.progressTrack.classList.remove("provider-queued");
      nodes.progressTrack.removeAttribute("aria-valuetext");
      const done = selectedItem()?.state === "done";
      nodes.progressTrack.setAttribute("aria-valuenow", done ? "100" : "0");
      nodes.progressFill.style.width = done ? "100%" : "0%";
      nodes.statusEta.textContent = "";
      return;
    }
    if (!item) return;
    const elapsedMs = Math.max(0, Date.now() - (item.operationStartedAt || Date.now()));
    const estimateMs = Math.max(10_000, item.estimatedTotalMs || 240_000);
    const providerQueued = item.result?.workspaceState === "provider_queue";
    nodes.progressTrack.classList.toggle("provider-queued", providerQueued);
    if (providerQueued) {
      nodes.progressTrack.removeAttribute("aria-valuenow");
      nodes.progressTrack.setAttribute("aria-valuetext", t("providerQueueEta"));
      nodes.progressFill.style.width = "34%";
      nodes.statusEta.textContent = t("providerQueueEta");
      return;
    }
    nodes.progressTrack.removeAttribute("aria-valuetext");
    const curved = Math.min(0.94, 0.9 * (1 - Math.exp((-3 * elapsedMs) / estimateMs)));
    const reported = Math.max(0, Math.min(0.94, item.result?.progressRatio || 0));
    item.displayedProgress = Math.max(item.displayedProgress || 0, curved, reported);
    const percent = Math.round(item.displayedProgress * 100);
    nodes.progressTrack.setAttribute("aria-valuenow", String(percent));
    nodes.progressFill.style.width = `${percent}%`;
    nodes.statusEta.textContent =
      elapsedMs >= estimateMs ? t("takingLonger") : this.formatRemaining(estimateMs - elapsedMs);
  }

  syncViewerControls() {
    const { nodes, selectedItem, state, viewer } = this.options;
    const hasModel = Boolean(selectedItem()?.loadedModelPath);
    nodes.viewerToolbar.classList.toggle("visible", hasModel);
    nodes.shadingButtons.forEach((button) => {
      const mode = button.dataset.shading as ShadingMode;
      button.classList.toggle("active", viewer.getShading() === mode);
      button.disabled = mode === "parts" && !viewer.hasParts();
    });
    nodes.outlineButton.classList.toggle("active", state.outline);
    nodes.rotateButton.classList.toggle("active", state.rotate);
    nodes.gridButton.classList.toggle("active", state.grid);
    nodes.wireButton.classList.toggle("active", state.wire);
  }

  updateUi() {
    const {
      state, nodes, selectedItem, activeJobCount, batchItems, isConfigurable, isDraft,
      normalizeSettings, maxParallelJobs, renderQueue, stripExtension,
    } = this.options;
    const item = selectedItem();
    const status = this.friendlyStatus();
    const busy = activeJobCount() > 0;
    const missing =
      item?.result?.runtimeStatus === "missing" || state.backendStatus.runtimeStatus === "missing";
    nodes.statusTitle.textContent = status.title;
    nodes.statusDetail.textContent = status.detail;
    nodes.stageStatus.dataset.stage = status.stage;
    nodes.statusMark.innerHTML = item?.state === "done" ? ICONS.check : ICONS.sparkle;
    nodes.readinessText.textContent = missing
      ? t("unavailable")
      : busy
        ? t("working")
        : state.preparationStatus === "ready"
          ? t("ready")
          : t("preparing");
    nodes.readiness.classList.toggle("busy", busy || state.preparationStatus === "preparing");
    nodes.readiness.classList.toggle("error", missing);
    nodes.sourceName.textContent = item ? stripExtension(item.name) : t("chooseImages");
    const selectedBatchSize = item ? batchItems(item.batchId).length : 0;
    nodes.sourceMeta.textContent = item
      ? selectedBatchSize > 1
        ? t("sharedSettings", { count: selectedBatchSize })
        : item.extension
      : t("formats");
    nodes.sourceThumb.innerHTML =
      item?.thumbnailUrl ? `<img alt="" src="${item.thumbnailUrl}">` : ICONS.image;
    nodes.folderName.textContent = state.outputDir || t("defaultFolder");
    nodes.folderName.title = state.outputDir;
    const settings = item
      ? normalizeSettings(item)
      : generationSettings("quality", 5000, false);
    nodes.polycountValue.value = new Intl.NumberFormat(locale()).format(settings.polycount);
    nodes.polycountRange.value = String(settings.polycount);
    nodes.polycountRange.min = String(settings.minimumPolycount);
    nodes.polycountRange.max = String(settings.maximumPolycount);
    nodes.modeButtons.forEach((button) => {
      const selected = button.dataset.generationMode === settings.mode;
      button.classList.toggle("active", selected);
      button.setAttribute("aria-pressed", String(selected));
    });
    nodes.autoSegmentSection.hidden = !settings.showAutoSegment;
    nodes.autoSegmentInput.checked = settings.autoSegment;
    const locked = !isConfigurable(item);
    nodes.modeButtons.forEach((button) => {
      button.disabled = locked;
    });
    nodes.polycountRange.disabled = locked;
    nodes.autoSegmentInput.disabled = locked;
    const selectedDraft = isDraft(item);
    nodes.generateButton.disabled = !item || missing || item.state === "running";
    nodes.generateButton.classList.toggle("is-busy", busy);
    const rerun =
      item ? item.state === "done" || item.state === "failed" || item.state === "cancelled" : false;
    nodes.generateLabel.textContent = selectedDraft && busy
      ? t("addToQueue")
      : item?.state === "running"
        ? t("creatingModel")
        : rerun
          ? t("generateAgain")
          : t("generateModel");
    nodes.cancelButton.classList.toggle("visible", busy || state.queueActive);
    nodes.cancelLabel.textContent =
      item?.result?.stage === "segmenting" ? t("cancelSegmentation") : t("cancel");
    const canSegment = item?.state === "done"
      && item.result?.canSegment
      && item.result?.jobId
      && !item.result?.isSegmented
      && activeJobCount() < maxParallelJobs;
    nodes.segmentButton.classList.toggle("visible", Boolean(canSegment));
    const hasModel = Boolean(item?.result?.outputPath && item.loadedModelPath);
    nodes.resultSummary.classList.toggle("visible", hasModel);
    nodes.resultName.textContent = item?.result?.isSegmented ? t("partsReady") : t("modelReady");
    nodes.resultMeta.textContent = item?.result?.outputName || t("savedAutomatically");
    const showModelStats = hasModel && Boolean(item?.modelStats);
    nodes.modelStats.textContent = item?.modelStats ? this.formatModelStats(item.modelStats) : "";
    nodes.modelStats.classList.toggle("visible", showModelStats);
    nodes.showFolderButton.classList.toggle("visible", Boolean(item?.result?.outputPath));
    nodes.emptyCopy.classList.toggle("hidden", Boolean(item));
    renderQueue();
    this.syncViewerControls();
    this.updateProgressUi();
  }

  private friendlyError(message: string) {
    const text = message.toLowerCase();
    if (text.includes("rate limit") || text.includes("retry after")) return t("serviceBusy");
    if (text.includes("runtime_missing") || text.includes("runtime") && text.includes("missing")) {
      return t("engineMissing");
    }
    if (text.includes("timed out") || text.includes("timeout")) return t("timedOut");
    if (text.includes("segment")) return t("separationFailed");
    return t("interrupted");
  }

  private friendlyStatus() {
    const { state, selectedItem, activeJobCount } = this.options;
    const item = selectedItem();
    if (!item) return { title: t("ready"), detail: t("chooseToBegin"), stage: "idle" as Stage };
    if (item.state === "done") {
      return {
        title: item.result?.isSegmented ? t("partsReady") : t("modelReady"),
        detail: item.result?.isSegmented ? t("dragInspectParts") : t("dragInspect"),
        stage: "done" as Stage,
      };
    }
    if (item.state === "failed") {
      return {
        title: t("couldNotCreate"),
        detail: this.friendlyError(item.result?.error || item.result?.progressText || ""),
        stage: "failed" as Stage,
      };
    }
    if (item.state === "cancelled") {
      return { title: t("cancelled"), detail: t("cancelledDetail"), stage: "cancelled" as Stage };
    }
    if (item.state === "queued" && item.submitted && activeJobCount()) {
      return { title: t("queuedTitle"), detail: t("queuedDetail"), stage: "idle" as Stage };
    }
    if (item.state !== "running") {
      return { title: t("ready"), detail: t("adjustThenGenerate"), stage: "idle" as Stage };
    }
    const status = item.result || state.backendStatus;
    if (status.workspaceState === "waiting") {
      return {
        title: t("preparingWorkspace"),
        detail: t("finishingPreparation"),
        stage: status.stage,
      };
    }
    if (status.workspaceState === "provider_queue") {
      return {
        title: t("providerQueueTitle"),
        detail: t("providerQueueDetail"),
        stage: status.stage,
      };
    }
    if (status.stage === "preparing") {
      return {
        title: t("preparingWorkspace"),
        detail: t("gettingEverythingReady"),
        stage: status.stage,
      };
    }
    if (status.stage === "segmenting") {
      return { title: t("separatingParts"), detail: t("findingPieces"), stage: status.stage };
    }
    if (status.stage === "finalizing") {
      return { title: t("finishingModel"), detail: t("preparingGeometry"), stage: status.stage };
    }
    const details: Record<string, MessageKey> = {
      depth_preview: "readingDepth",
      model_setup: "preparingImage",
      model_creation: "shapingGeometry",
      separation: "findingPieces",
      finalizing: "preparingGeometry",
    };
    return {
      title: t("creatingModel"),
      detail: t(details[status.phase || ""] || (status.previewPath ? "shapingGeometry" : "readingDepth")),
      stage: status.stage,
    };
  }

  private formatRemaining(milliseconds: number) {
    if (milliseconds <= 15_000) return t("almostThere");
    if (milliseconds < 60_000) return t("lessMinute");
    return t("aboutMinutes", { count: Math.max(1, Math.ceil(milliseconds / 60_000)) });
  }

  private formatModelStats(stats: ModelStats) {
    const number = new Intl.NumberFormat(locale());
    return t("modelStats", {
      vertices: number.format(stats.vertices),
      faces: number.format(stats.faces),
    });
  }
}
