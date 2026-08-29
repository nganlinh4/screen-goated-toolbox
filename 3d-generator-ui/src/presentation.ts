import { generationSettings } from "./generation-mode";
import { locale, t, type MessageKey } from "./i18n";
import { ICONS } from "./layout";
import type { AppNodes, AppState, QueueItem, Stage } from "./types";
import type { ModelViewer, ModelStats, ShadingMode } from "./viewer";
import { canSubmitItem } from "./submission-policy";
import { declaredRefinements } from "./refinement-policy";
import {
  DEFAULT_PROGRESS_RANGE,
  nextDisplayedProgress,
} from "./progress-policy";

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
  renderQueue: () => void;
  stripExtension: (name: string) => string;
};

export class ModelPresentation {
  private sourceThumbnailUrl = "";
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
    document.querySelectorAll<HTMLElement>("[data-i18n-placeholder]").forEach((node) => {
      node.setAttribute("placeholder", t(node.dataset.i18nPlaceholder as MessageKey));
    });
    const { state, nodes, stripExtension } = this.options;
    const referenceItem = state.items.find((item) => item.id === state.referencePreviewItemId);
    if (referenceItem) {
      nodes.referencePreviewImage.alt = t("referenceImageAlt", {
        name: stripExtension(referenceItem.name),
      });
    }
  }

  beginProgress(item: QueueItem, estimateMs: number, range = DEFAULT_PROGRESS_RANGE) {
    item.operationStartedAt = Date.now();
    item.estimatedTotalMs = estimateMs;
    item.progressRange = range;
    item.progressContinues = false;
    item.displayedProgress = range.start;
  }

  updateProgressUi() {
    const { nodes, selectedItem } = this.options;
    const item = selectedItem();
    nodes.controlRail.classList.toggle("project-mode", item?.state === "done");
    const busy = item?.state === "running";
    nodes.progressTrack.classList.toggle("visible", busy);
    nodes.statusEta.classList.toggle("visible", busy);
    if (!busy) {
      nodes.progressTrack.removeAttribute("aria-valuetext");
      const selected = selectedItem();
      const done = selected?.state === "done";
      const continuedPercent = Math.round((selected?.displayedProgress || 0) * 100);
      nodes.progressTrack.setAttribute(
        "aria-valuenow",
        String(selected?.progressContinues ? continuedPercent : done ? 100 : 0),
      );
      if (!selected?.progressContinues) {
        nodes.progressFill.style.width = done ? "100%" : "0%";
      }
      nodes.statusEta.textContent = "";
      return;
    }
    if (!item) return;
    const elapsedMs = Math.max(0, Date.now() - (item.operationStartedAt || Date.now()));
    const estimateMs = Math.max(10_000, item.estimatedTotalMs || 240_000);
    nodes.progressTrack.removeAttribute("aria-valuetext");
    item.displayedProgress = nextDisplayedProgress(
      item.displayedProgress || 0,
      elapsedMs,
      estimateMs,
      item.result?.progressRatio || 0,
      item.progressRange || DEFAULT_PROGRESS_RANGE,
    );
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
      normalizeSettings, renderQueue, stripExtension,
    } = this.options;
    const item = selectedItem();
    const status = this.friendlyStatus();
    const busy = activeJobCount() > 0;
    const missing =
      item?.result?.runtimeStatus === "missing" || state.selectedStatus.runtimeStatus === "missing";
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
    this.syncSourceThumbnail(item?.thumbnailUrl);
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
    const instructionAvailable =
      state.generationCapabilities.ready
      && state.generationCapabilities.optionalInstruction[settings.mode];
    nodes.instructionSection.hidden = !instructionAvailable;
    if (nodes.instructionInput.value !== (item?.instruction || "")) {
      nodes.instructionInput.value = item?.instruction || "";
    }
    const locked = !isConfigurable(item);
    nodes.modeButtons.forEach((button) => {
      button.disabled = locked;
    });
    nodes.polycountRange.disabled = locked;
    nodes.autoSegmentInput.disabled = locked;
    nodes.instructionInput.disabled = locked;
    const selectedDraft = isDraft(item);
    nodes.generateButton.disabled = missing || !canSubmitItem(item);
    nodes.generateButton.classList.toggle("is-busy", busy);
    const rerun =
      item ? item.state === "done" || item.state === "failed" || item.state === "cancelled" : false;
    nodes.generateLabel.textContent = selectedDraft && busy
      ? t("addToQueue")
      : item?.state === "running" || item?.state === "queued" && item.submitted
        ? t("generateAgain")
        : rerun
          ? t("generateAgain")
          : t("generateModel");
    nodes.cancelButton.classList.toggle("visible", busy || state.queueActive);
    nodes.cancelLabel.textContent =
      item?.result?.stage === "segmenting" ? t("cancelSegmentation") : t("cancel");
    nodes.segmentButton.classList.remove("visible");
    const hasModel = Boolean(item?.result?.outputPath && item.loadedModelPath);
    const supported = declaredRefinements(
      item?.result?.supportedActions,
      item?.result?.availableActions,
    );
    const showRefinements = Boolean(
      hasModel && item?.state === "done" && item.result?.jobId && supported.size,
    );
    const available = new Set(item?.result?.availableActions || []);
    nodes.refinementPanel.classList.toggle("visible", showRefinements);
    nodes.refinementButtons.forEach((button) => {
      const action = button.dataset.refinement;
      const capability = action === "optimize_mesh"
        ? `optimize_${nodes.topologySelect.value}`
        : action;
      const actionSupported = action === "optimize_mesh"
        ? supported.has("optimize_triangle") || supported.has("optimize_quad")
        : Boolean(action && supported.has(action));
      button.hidden = !actionSupported;
      button.disabled = !showRefinements || !capability || !available.has(capability);
      const row = button.closest<HTMLElement>(".refinement-row");
      if (row) row.hidden = !actionSupported;
    });
    const standaloneActions = nodes.refinementPanel.querySelector<HTMLElement>(
      ".refinement-actions",
    );
    if (standaloneActions) {
      standaloneActions.hidden = ![...standaloneActions.querySelectorAll("button")]
        .some((button) => !(button as HTMLButtonElement).hidden);
    }
    [...nodes.topologySelect.options].forEach((option) => {
      const action = `optimize_${option.value}`;
      option.hidden = !supported.has(action);
      option.disabled = !supported.has(action);
    });
    if (nodes.topologySelect.selectedOptions[0]?.disabled) {
      const firstSupported = [...nodes.topologySelect.options].find((option) => !option.disabled);
      if (firstSupported) nodes.topologySelect.value = firstSupported.value;
    }
    const separateAvailable = available.has("separate_parts");
    const optimizeAvailable = available.has(`optimize_${nodes.topologySelect.value}`);
    const animateAvailable = available.has("animate");
    nodes.segmentationLevel.disabled = !showRefinements || !separateAvailable;
    nodes.topologySelect.disabled = !showRefinements
      || !available.has("optimize_triangle") && !available.has("optimize_quad");
    nodes.faceLimitInput.disabled = !showRefinements || !optimizeAvailable;
    nodes.animationSelect.disabled = !showRefinements || !animateAvailable;
    nodes.resultSummary.classList.toggle("visible", hasModel);
    nodes.resultName.textContent = item?.result?.isSegmented ? t("partsReady") : t("modelReady");
    nodes.resultMeta.textContent = item?.exportedNames?.length
      ? t("downloadedFiles", { count: item.exportedNames.length })
      : t("projectAutosaved");
    nodes.downloadButton.classList.toggle("visible", hasModel);
    nodes.downloadButton.disabled = !hasModel || item?.state !== "done";
    nodes.downloadLabel.textContent = t("downloadCurrent");
    const showModelStats = hasModel && Boolean(item?.modelStats);
    nodes.modelStats.textContent = item?.modelStats ? this.formatModelStats(item.modelStats) : "";
    nodes.modelStats.classList.toggle("visible", showModelStats);
    nodes.emptyCopy.classList.toggle("hidden", Boolean(item));
    renderQueue();
    this.syncViewerControls();
    this.updateProgressUi();
  }

  private syncSourceThumbnail(url?: string) {
    const next = url || "";
    if (next === this.sourceThumbnailUrl && this.options.nodes.sourceThumb.childElementCount) return;
    this.sourceThumbnailUrl = next;
    if (!next) {
      this.options.nodes.sourceThumb.innerHTML = ICONS.image;
      return;
    }
    const image = document.createElement("img");
    image.alt = "";
    image.src = next;
    this.options.nodes.sourceThumb.replaceChildren(image);
  }

  private friendlyError(code?: string | null) {
    switch (code) {
      case "engine_unavailable": return t("toolUnavailable");
      case "timed_out": return t("timedOut");
      case "separation_failed": return t("separationFailed");
      default: return t("interrupted");
    }
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
        detail: this.friendlyError(item.result?.error),
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
    const status = item.result || state.selectedStatus;
    if (status.stage === "preparing") {
      return {
        title: t("preparing"),
        detail: t("gettingEverythingReady"),
        stage: status.stage,
      };
    }
    if (status.stage === "segmenting") {
      return { title: t("separatingParts"), detail: t("findingPieces"), stage: status.stage };
    }
    if (status.stage === "refining") {
      return { title: t("creating"), detail: t("preparingGeometry"), stage: status.stage };
    }
    if (status.stage === "finalizing") {
      return { title: t("finishingModel"), detail: t("preparingGeometry"), stage: status.stage };
    }
    const details: Record<string, MessageKey> = {
      model_setup: "preparingImage",
      model_creation: "shapingGeometry",
      separation: "findingPieces",
      finalizing: "preparingGeometry",
    };
    return {
      title: t("creatingModel"),
      detail: t(details[status.phase || ""] || "preparingImage"),
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
    // A quad model's faces are polygons; reporting its triangle count would
    // describe the render, not the mesh the file contains.
    if (stats.polygons !== undefined && stats.quads !== undefined) {
      return t("modelStatsQuads", {
        vertices: number.format(stats.vertices),
        polygons: number.format(stats.polygons),
        quads: number.format(stats.quads),
      });
    }
    return t("modelStats", {
      vertices: number.format(stats.vertices),
      faces: number.format(stats.faces),
    });
  }
}
