import { setLocale } from "./i18n";
import type { GenerationMode } from "./generation-mode";
import type { ShadingMode, ModelViewer } from "./viewer";
import type { AppNodes, AppState, ExportedResult, HostContext, JobStatus, QueueItem } from "./types";
import { t } from "./i18n";
import type { ModelPresentation } from "./presentation";
import type { JobRunner } from "./job-runner";

type BindControlOptions = {
  state: AppState;
  nodes: AppNodes;
  viewer: ModelViewer;
  presentation: ModelPresentation;
  jobRunner: JobRunner;
  invoke: <T = unknown>(cmd: string, args?: unknown) => Promise<T>;
  selectedItem: () => QueueItem | undefined;
  isConfigurable: (item?: QueueItem) => boolean;
  isRerunnable: (item?: QueueItem) => boolean;
  batchItems: (batchId: string) => QueueItem[];
  normalizeSettings: (item: QueueItem) => unknown;
  updateUi: () => void;
  addImages: () => Promise<void>;
  addImagePaths: (paths: string[]) => Promise<void>;
  closeReferencePreview: () => void;
  notify: (message: string) => void;
};

export function bindControls(options: BindControlOptions) {
  const {
    state, nodes, viewer, presentation, jobRunner, invoke, selectedItem,
    isConfigurable, isRerunnable, batchItems, normalizeSettings, updateUi,
    addImages, addImagePaths, closeReferencePreview, notify,
  } = options;

  window.applyHostContext = (context: HostContext) => {
    const theme = context.theme === "light" ? "light" : "dark";
    document.documentElement.dataset.theme = theme;
    setLocale(context.language);
    viewer.setTheme(theme);
    presentation.applyTranslations();
    updateUi();
  };
  window.handleNativeFileDrag = (active) => document.body.classList.toggle("file-dragging", active);
  window.handleNativeFileDrop = (paths) => {
    document.body.classList.remove("file-dragging");
    void addImagePaths(paths);
  };
  nodes.dragRegion.addEventListener("pointerdown", (event) => {
    if ((event.target as HTMLElement).closest("button, input, label")) return;
    void invoke("start_drag");
  });
  nodes.minimizeButton.addEventListener("click", () => void invoke("minimize_window"));
  nodes.closeButton.addEventListener("click", () => void invoke("close_window"));
  nodes.addImagesButton.addEventListener("click", () => void addImages());
  nodes.chooseImageButton.addEventListener("click", () => void addImages());
  nodes.downloadButton.addEventListener("click", async () => {
    const item = selectedItem();
    const outputPath = item?.result?.outputPath;
    if (!item || !outputPath || nodes.downloadButton.disabled) return;
    nodes.downloadButton.disabled = true;
    nodes.downloadLabel.textContent = t("downloading");
    try {
      const exported = await invoke<ExportedResult>("export_result", { outputPath });
      item.exportedNames = exported.names;
      notify(t("downloadedToDownloads"));
    } catch {
      notify(t("downloadFailed"));
    } finally {
      updateUi();
    }
  });
  nodes.generateButton.addEventListener("click", () => jobRunner.submitSelected());
  nodes.segmentButton.addEventListener("click", () => void jobRunner.segmentSelected());
  nodes.refinementButtons.forEach((button) => button.addEventListener("click", () => {
    const action = button.dataset.refinement;
    if (!action || button.disabled) return;
    if (action === "separate_parts") {
      void jobRunner.refineSelected({
        kind: "separate_parts",
        segmentationLevel: nodes.segmentationLevel.value as "simple" | "balanced" | "detailed",
      });
    } else if (action === "optimize_mesh") {
      void jobRunner.refineSelected({
        kind: "optimize_mesh",
        topology: nodes.topologySelect.value as "triangle" | "quad",
        faceLimit: Number(nodes.faceLimitInput.value),
      });
    } else if (action === "animate") {
      void jobRunner.refineSelected({
        kind: "animate",
        animation: nodes.animationSelect.value as "idle" | "walk" | "run" | "jump" | "wave_goodbye_01",
      });
    } else if (action === "add_materials" || action === "generate_pbr" || action === "rig") {
      void jobRunner.refineSelected({ kind: action });
    }
  }));
  nodes.topologySelect.addEventListener("change", updateUi);
  nodes.cancelButton.addEventListener("click", async () => {
    const item = selectedItem();
    if (!item) return;
    item.cancelRequested = true;
    if (item.result?.jobId) {
      const status = await invoke<JobStatus>("cancel_job", { jobId: item.result.jobId });
      item.result = status;
      item.state = "cancelled";
      state.runningIds.delete(item.id);
      updateUi();
      return;
    }
    item.state = "cancelled";
    state.runningIds.delete(item.id);
    updateUi();
  });
  nodes.modeButtons.forEach((button) => button.addEventListener("click", () => {
    const item = selectedItem();
    const generationMode = button.dataset.generationMode as GenerationMode;
    if (!item || !isConfigurable(item) || !generationMode) return;
    const update = (member: QueueItem) => {
      member.generationMode = generationMode;
      normalizeSettings(member);
    };
    if (isRerunnable(item)) update(item);
    else {
      batchItems(item.batchId).forEach((member) => {
        if (member.state === "queued" && !member.submitted) update(member);
      });
    }
    updateUi();
  }));
  nodes.polycountRange.addEventListener("input", () => {
    const item = selectedItem();
    if (!item || !isConfigurable(item)) return;
    const value = Number(nodes.polycountRange.value);
    const update = (member: QueueItem) => {
      member.polycount = value;
      normalizeSettings(member);
    };
    if (isRerunnable(item)) update(item);
    else {
      batchItems(item.batchId).forEach((member) => {
        if (member.state === "queued" && !member.submitted) update(member);
      });
    }
    updateUi();
  });
  nodes.autoSegmentInput.addEventListener("change", () => {
    const item = selectedItem();
    if (!item || !isConfigurable(item)) return;
    const update = (member: QueueItem) => {
      member.autoSegment = nodes.autoSegmentInput.checked;
      normalizeSettings(member);
    };
    if (isRerunnable(item)) update(item);
    else {
      batchItems(item.batchId).forEach((member) => {
        if (member.state === "queued" && !member.submitted) update(member);
      });
    }
    updateUi();
  });
  nodes.instructionInput.addEventListener("input", () => {
    const item = selectedItem();
    if (!item || !isConfigurable(item)) return;
    const value = nodes.instructionInput.value;
    const update = (member: QueueItem) => {
      member.instruction = value;
    };
    if (isRerunnable(item)) update(item);
    else {
      batchItems(item.batchId).forEach((member) => {
        if (member.state === "queued" && !member.submitted) update(member);
      });
    }
  });
  nodes.shadingButtons.forEach((button) => button.addEventListener("click", () => {
    viewer.setShading(button.dataset.shading as ShadingMode);
    presentation.syncViewerControls();
  }));
  nodes.outlineButton.addEventListener("click", () => {
    state.outline = !state.outline;
    viewer.setOutline(state.outline);
    presentation.syncViewerControls();
  });
  nodes.rotateButton.addEventListener("click", () => {
    state.rotate = !state.rotate;
    viewer.setAutoRotate(state.rotate);
    presentation.syncViewerControls();
  });
  nodes.gridButton.addEventListener("click", () => {
    state.grid = !state.grid;
    viewer.setGrid(state.grid);
    presentation.syncViewerControls();
  });
  nodes.wireButton.addEventListener("click", () => {
    state.wire = !state.wire;
    viewer.setWireframe(state.wire);
    presentation.syncViewerControls();
  });
  nodes.fitButton.addEventListener("click", () => viewer.fitView());
  nodes.referencePreviewClose.addEventListener("click", closeReferencePreview);
  window.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && !nodes.referencePreview.hidden) closeReferencePreview();
  });
}
