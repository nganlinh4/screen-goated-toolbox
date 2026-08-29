import "./styles.css";
import "./stage.css";
import "../../ui-shared/creation-shell-layout.css";
import { confirmDestructive } from "../../ui-shared/destructive-confirmation";
import { setLocale, t } from "./i18n";
import { generationSettings } from "./generation-mode";
import { ModelViewer } from "./viewer";
import { appMarkup } from "./layout";
import { collectNodes } from "./dom";
import { ModelQueueView } from "./queue-view";
import { ModelPresentation } from "./presentation";
import { JobRunner } from "./job-runner";
import { DevHarness } from "./dev-harness";
import { bindControls } from "./bind-controls";
import { frozenGenerationSettings } from "./recovery-settings";
import { ModelDisplayLane } from "./model-display";
import { LatestOnlyLane } from "./latest-only-lane";
import { applyHistoryRevision, historyRevision } from "./history-status";
import { ImagePreviewCache, normalizedProjectThumbnail } from "./image-preview-cache";
import type {
  AppState,
  AssetPayload,
  HistoryEntry,
  HostContext,
  JobStatus,
  QueueItem,
} from "./types";

declare global {
  interface Window {
    invoke?: <T = unknown>(cmd: string, args?: unknown) => Promise<T>;
    ipc?: { postMessage: (message: string) => void };
    __SGT_CONTEXT__?: HostContext;
    __SGT_PARALLEL_TEST__?: {
      starts: string[];
      active: number;
      maxActive: number;
      completed: number;
    };
    applyHostContext?: (context: HostContext) => void;
    handleNativeFileDrop?: (paths: string[]) => void;
    handleNativeFileDrag?: (active: boolean) => void;
  }
}

const BUSY_STAGES = new Set<JobStatus["stage"]>([
  "queued", "preparing", "generating", "segmenting", "refining", "finalizing",
]);
const MAX_IMPORT_SESSIONS = 100;
const initialContext = window.__SGT_CONTEXT__ || {};
const devParams = import.meta.env.DEV ? new URLSearchParams(window.location.search) : null;
setLocale(devParams?.get("lang") || initialContext.language);
document.documentElement.dataset.theme =
  devParams?.get("theme")
  || initialContext.theme
  || document.documentElement.dataset.theme
  || "dark";
document.documentElement.dataset.creationShellOnly = "false";

function invoke<T = unknown>(cmd: string, args: unknown = {}): Promise<T> {
  if (window.invoke) return window.invoke<T>(cmd, args);
  return Promise.reject(new Error("The desktop bridge is not available."));
}

const app = document.querySelector<HTMLElement>("#app");
if (!app) throw new Error("App root not found");
app.innerHTML = appMarkup();
const nodes = collectNodes();
const viewer = new ModelViewer(nodes.canvas, nodes.stage);
const modelDisplay = new ModelDisplayLane(viewer, invoke);
const imagePreviews = new ImagePreviewCache(invoke);
const referencePreviewLane = new LatestOnlyLane<AssetPayload>();
window.addEventListener("pagehide", () => {
  referencePreviewLane.invalidate();
  imagePreviews.clear();
  modelDisplay.dispose();
}, { once: true });
const state: AppState = {
  items: [],
  selectedId: "",
  runningIds: new Set<string>(),
  outputDir: "",
  queueActive: false,
  cancelRequested: false,
  selectedStatus: { stage: "idle", progressText: "", runtimeStatus: "checking" },
  preparationStatus: "ready",
  displayToken: 0,
  displayedItemId: "",
  displayedModelPath: "",
  displayRequestKey: "",
  displayPromise: undefined,
  outline: true,
  rotate: false,
  grid: false,
  wire: false,
  historyRefreshing: false,
  referencePreviewItemId: "",
  referencePreviewToken: 0,
  generationCapabilities: {
    ready: false,
    optionalInstruction: { fast: false, quality: false },
  },
};
function pathLeaf(path: string) {
  return path.split(/[\\/]/).filter(Boolean).pop() || path;
}
function pathParent(path: string) {
  const separator = Math.max(path.lastIndexOf("\\"), path.lastIndexOf("/"));
  return separator > 0 ? path.slice(0, separator) : "";
}
function stripExtension(name: string) {
  return name.replace(/\.[^.]+$/, "");
}
function selectedItem() {
  return state.items.find((item) => item.id === state.selectedId);
}
function batchItems(batchId: string) {
  return state.items.filter((item) => item.batchId === batchId);
}
function activeJobCount() {
  return state.runningIds.size;
}
function isDraft(item?: QueueItem) {
  return Boolean(item?.state === "queued" && !item.submitted);
}

function isRerunnable(item?: QueueItem) {
  return Boolean(
    item
      && ["done", "failed", "cancelled"].includes(item.state)
      && item.sourceProvenance === "surface-import",
  );
}

function isConfigurable(item?: QueueItem) {
  return isDraft(item) || isRerunnable(item);
}

function normalizeGenerationSettings(item: QueueItem) {
  const settings = generationSettings(item.generationMode, item.polycount, item.autoSegment);
  item.generationMode = settings.mode;
  item.polycount = settings.polycount;
  item.autoSegment = settings.autoSegment;
  return settings;
}

let toastTimer = 0;

function showToast(message: string) {
  window.clearTimeout(toastTimer);
  nodes.appToast.textContent = message;
  nodes.appToast.classList.add("visible");
  toastTimer = window.setTimeout(() => nodes.appToast.classList.remove("visible"), 4_200);
}

function closeReferencePreview() {
  referencePreviewLane.invalidate();
  state.referencePreviewToken += 1;
  state.referencePreviewItemId = "";
  nodes.referencePreview.hidden = true;
  nodes.referencePreviewImage.removeAttribute("src");
}

async function openReferencePreview(item: QueueItem) {
  if (state.selectedId !== item.id) void selectItem(item.id);
  const token = ++state.referencePreviewToken;
  state.referencePreviewItemId = item.id;
  nodes.referencePreviewName.textContent = stripExtension(item.name);
  nodes.referencePreviewImage.alt = t("referenceImageAlt", { name: stripExtension(item.name) });
  nodes.referencePreview.hidden = false;
  try {
    const preview = await referencePreviewLane.run(
      () => readImagePreview(item.path, 1_600),
      () => undefined,
    );
    if (!preview) return;
    const previewUrl = preview.dataUrl;
    if (token !== state.referencePreviewToken || state.referencePreviewItemId !== item.id) return;
    nodes.referencePreviewImage.src = previewUrl;
  } catch {
    if (token !== state.referencePreviewToken) return;
    closeReferencePreview();
    showToast(t("referenceUnavailable"));
  }
}

async function readImagePreview(path: string, maxEdge: number) {
  return imagePreviews.load(path, maxEdge);
}

function comparablePath(path?: string | null) {
  return (path || "").toLowerCase();
}

let presentation!: ModelPresentation;
let jobRunner!: JobRunner;
const queueView = new ModelQueueView({
  state,
  nodes,
  batchItems,
  stripExtension,
  onSelect: (id) => void selectItem(id),
  onOpenReference: (item) => void openReferencePreview(item),
  onRemove: removeItem,
  onRename: (item, name) => void renameHistoryItem(item, name),
  onDelete: (item) => void deleteHistoryItem(item),
  onThumbnailNeeded: (item) => imagePreviews.ensureProjectThumbnail(item, updateUi),
});
window.addEventListener("pagehide", () => queueView.dispose(), { once: true });
presentation = new ModelPresentation({
  state,
  nodes,
  viewer,
  selectedItem,
  batchItems,
  activeJobCount,
  isConfigurable,
  isDraft,
  normalizeSettings: normalizeGenerationSettings,
  renderQueue: () => queueView.render(),
  stripExtension,
});

function updateUi() {
  presentation.updateUi();
}

async function renameHistoryItem(item: QueueItem, newName: string) {
  if (!item.historyId) return;
  try {
    const entry = await invoke<HistoryEntry>("rename_history_result", {
      id: item.historyId,
      newName,
    });
    if (item.result) {
      item.result.outputPath = entry.outputPath;
      item.result.outputName = entry.outputName;
      item.result.downloadPath = entry.metadata?.download?.path;
      item.result.downloadName = entry.metadata?.download?.name;
    }
  } catch {
    showToast(t("renameFailed"));
  } finally {
    queueView.finishRename();
    updateUi();
  }
}

async function deleteHistoryItem(item: QueueItem) {
  if (!item.historyId) return;
  try {
    await invoke("delete_history_result", { id: item.historyId });
    removeItem(item.id);
  } catch {
    showToast(t("deleteFailed"));
  }
}

async function deleteAllHistory() {
  if (!await confirmDestructive({
    message: t("deleteAllConfirm"),
    confirmLabel: t("deleteAll"),
    cancelLabel: t("cancel"),
    cancelClass: "secondary-action",
  })) return;
  try {
    await invoke("delete_all_history_results");
    state.items = state.items.filter((item) => !item.historyId);
    if (!state.items.some((item) => item.id === state.selectedId)) {
      state.selectedId = state.items[0]?.id || "";
    }
    closeReferencePreview();
    updateUi();
    const item = selectedItem();
    if (item) await displayItem(item);
  } catch {
    showToast(t("deleteFailed"));
  }
}

async function refreshHistory() {
  if (state.historyRefreshing || !window.invoke) return;
  state.historyRefreshing = true;
  try {
    const entries = await invoke<HistoryEntry[]>("history_results");
    const validIds = new Set(entries.map((entry) => entry.id));
    for (const entry of entries) {
      const frozen = frozenGenerationSettings(entry.metadata || {});
      const legacy = generationSettings(
        entry.metadata?.generationMode || "quality",
        Number.NaN,
        false,
      );
      const settings = frozen || {
        generationMode: legacy.mode,
        polycount: legacy.polycount,
        autoSegment: legacy.autoSegment,
        instruction: undefined,
        outputDir: pathParent(entry.outputPath),
      };
      let item = state.items.find((candidate) => candidate.historyId === entry.id)
        || state.items.find((candidate) =>
          comparablePath(candidate.result?.outputPath) === comparablePath(entry.outputPath));
      if (item) {
        item.historyId = entry.id;
        item.createdAtMs = entry.createdAtMs;
        item.generationMode = settings.generationMode;
        item.polycount = settings.polycount;
        item.autoSegment = settings.autoSegment;
        item.instruction = settings.instruction;
        item.outputDir = settings.outputDir;
        item.thumbnailUrl ||= normalizedProjectThumbnail(entry.metadata?.projectThumbnail);
        if (item.result) {
          item.result.outputPath = entry.outputPath;
          item.result.outputName = entry.outputName;
          item.result.downloadPath = entry.metadata?.download?.path;
          item.result.downloadName = entry.metadata?.download?.name;
          item.result.isSegmented = Boolean(entry.metadata?.isSegmented);
          applyHistoryRevision(entry, item.result);
          item.result.outputDir = settings.outputDir;
          item.result.generationMode = settings.generationMode;
          item.result.polycount = settings.polycount;
          item.result.autoSegment = settings.autoSegment;
          item.result.instruction = settings.instruction;
        }
        continue;
      }
      const sourcePath = entry.sourcePath;
      const name = pathLeaf(sourcePath || entry.outputName);
      state.items.push({
        id: `history_${entry.id}`,
        batchId: entry.metadata?.projectId || `history_${entry.id}`,
        path: sourcePath,
        sourceProvenance: "presentation",
        name,
        extension: name.split(".").pop()?.toUpperCase() || t("image"),
        thumbnailUrl: normalizedProjectThumbnail(entry.metadata?.projectThumbnail),
        generationMode: settings.generationMode,
        polycount: settings.polycount,
        autoSegment: settings.autoSegment,
        instruction: settings.instruction,
        outputDir: settings.outputDir,
        submitted: true,
        state: "done",
        historyId: entry.id,
        createdAtMs: entry.createdAtMs,
        result: {
          stage: "done",
          progressText: "",
          outputPath: entry.outputPath,
          outputName: entry.outputName,
          downloadPath: entry.metadata?.download?.path,
          downloadName: entry.metadata?.download?.name,
          sourceImagePath: sourcePath,
          outputDir: settings.outputDir,
          generationMode: settings.generationMode,
          polycount: settings.polycount,
          autoSegment: settings.autoSegment,
          instruction: settings.instruction,
          isSegmented: Boolean(entry.metadata?.isSegmented),
          ...historyRevision(entry),
          canRefine: false,
          canSegment: false,
        },
      });
    }
    const selectedBefore = state.selectedId;
    state.items = state.items.filter((item) => !item.historyId || validIds.has(item.historyId));
    if (
      state.referencePreviewItemId
      && !state.items.some((item) => item.id === state.referencePreviewItemId)
    ) closeReferencePreview();
    if (!state.items.some((item) => item.id === state.selectedId)) {
      state.selectedId = state.items[0]?.id || "";
    }
    updateUi();
    if (state.selectedId && state.selectedId !== selectedBefore) {
      const item = selectedItem();
      if (item) await displayItem(item);
    }
  } catch {
    // Keep the active queue usable if history storage is unavailable.
  } finally {
    state.historyRefreshing = false;
  }
}

async function addImagePaths(paths: string[]) {
  if (!paths.length) return;
  if (paths.length > MAX_IMPORT_SESSIONS) {
    showToast(t("importLimit", { count: MAX_IMPORT_SESSIONS }));
    return;
  }
  const accepted = paths.map((path) => path.trim()).filter(Boolean);
  if (!accepted.length) return;
  const batchId = `batch_${Date.now()}_${Math.random().toString(36).slice(2)}`;
  const created = accepted.map((path, index): QueueItem => {
    const name = pathLeaf(path);
    return {
      id: `image_${Date.now()}_${Math.random().toString(36).slice(2)}`,
      batchId,
      path,
      sourceProvenance: "surface-import",
      name,
      extension: name.split(".").pop()?.toUpperCase() || t("image"),
      generationMode: "quality",
      polycount: 5_000,
      autoSegment: false,
      submitted: false,
      state: "queued",
      createdAtMs: Date.now() - index,
    };
  });
  closeReferencePreview();
  state.items.push(...created);
  state.selectedId = created[0].id;
  updateUi();
  void displayItem(created[0]);
}

async function addImages() {
  await addImagePaths(await invoke<string[]>("pick_images"));
}

function removeItem(id: string) {
  const index = state.items.findIndex((item) => item.id === id);
  if (index < 0 || state.items[index].state === "running") return;
  if (state.referencePreviewItemId === id) closeReferencePreview();
  state.items.splice(index, 1);
  if (state.selectedId === id) {
    state.selectedId = state.items[Math.min(index, state.items.length - 1)]?.id || "";
  }
  updateUi();
  const item = selectedItem();
  if (item) void displayItem(item);
}

async function selectItem(id: string) {
  if (state.referencePreviewItemId && state.referencePreviewItemId !== id) closeReferencePreview();
  state.selectedId = id;
  updateUi();
  const item = selectedItem();
  if (item) await displayItem(item);
}

function displayItem(item: QueueItem): Promise<void> {
  const modelPath = item.result?.outputPath
    && (
      item.state === "done"
      || item.result.stage === "done"
      || item.result.stage === "segmenting"
      || item.state === "failed"
      || item.state === "cancelled"
    )
    ? item.result.outputPath
    : undefined;
  if (
    modelPath
    && state.displayedItemId === item.id
    && state.displayedModelPath === modelPath
    && item.loadedModelPath === modelPath
  ) {
    presentation.syncViewerControls();
    return Promise.resolve();
  }
  const requestKey = `${item.id}\n${modelPath || "source"}\n${Boolean(item.result?.isSegmented)}`;
  if (state.displayRequestKey === requestKey && state.displayPromise) return state.displayPromise;
  const token = ++state.displayToken;
  const operation = (async () => {
    try {
      const outcome = await modelDisplay.display(
        modelPath,
        Boolean(item.result?.isSegmented),
      );
      if (!outcome || token !== state.displayToken || state.selectedId !== item.id) return;
      if (outcome.kind === "model" && modelPath) {
        item.modelStats = outcome.stats;
        item.loadedModelPath = modelPath;
        state.displayedItemId = item.id;
        state.displayedModelPath = modelPath;
        updateUi();
        return;
      }
      state.displayedItemId = item.id;
      state.displayedModelPath = "";
      item.loadedModelPath = "";
    } catch {
      // The status surface remains usable even if preview loading fails.
    }
    presentation.syncViewerControls();
  })();
  state.displayRequestKey = requestKey;
  state.displayPromise = operation;
  void operation.finally(() => {
    if (state.displayPromise === operation) {
      state.displayPromise = undefined;
      state.displayRequestKey = "";
    }
  });
  return operation;
}

jobRunner = new JobRunner({
  state,
  busyStages: BUSY_STAGES,
  invoke,
  normalizeSettings: normalizeGenerationSettings,
  selectedItem,
  pathLeaf,
  displayItem,
  refreshHistory,
  updateUi,
  beginProgress: (item, estimateMs, range) => presentation.beginProgress(item, estimateMs, range),
});

if (import.meta.env.DEV) {
  // Development-only handle so a headless or backgrounded tab, where
  // requestAnimationFrame never fires, can still be driven to draw a frame.
  (window as unknown as Record<string, unknown>).__sgtViewer = viewer;
}

const devHarness = devParams
  ? new DevHarness({
    state,
    viewer,
    params: devParams,
    pathLeaf,
    updateUi,
    processQueue: () => void jobRunner.processQueue(),
  })
  : undefined;

bindControls({
  state,
  nodes,
  viewer,
  presentation,
  jobRunner,
  invoke,
  selectedItem,
  isConfigurable,
  isRerunnable,
  batchItems,
  normalizeSettings: normalizeGenerationSettings,
  updateUi,
  addImages,
  addImagePaths,
  closeReferencePreview, notify: showToast,
});
document.querySelector("#deleteAllHistory")
  ?.addEventListener("click", () => void deleteAllHistory());

async function loadDefaultOutputDir() {
  try {
    state.outputDir = await invoke<string>("default_output_dir");
    updateUi();
  } catch {
    // Standalone previews have no selected output directory.
  }
}

async function refreshGenerationCapabilities() {
  try {
    state.generationCapabilities = await invoke("generation_capabilities");
    updateUi();
  } catch {
    state.generationCapabilities = {
      ready: false,
      optionalInstruction: { fast: false, quality: false },
    };
  }
}

presentation.applyTranslations();
updateUi();
const devModelUrl = devParams?.get("model");
if (devParams?.get("output")) {
  state.outputDir = devParams.get("output") || "";
  updateUi();
}
if (devParams?.get("parallel") === "1") {
  devHarness?.loadParallelHarness();
} else if (devParams?.get("batches") === "1") {
  devHarness?.loadBatchPreview();
} else if (devModelUrl) {
  void devHarness?.loadModelPreview(devModelUrl);
} else if (window.invoke) {
  void (async () => {
    void invoke("prepare_runtime").catch(() => undefined);
    await loadDefaultOutputDir();
    await jobRunner.restoreCurrentJobs();
    await refreshHistory();
    void refreshGenerationCapabilities();
    window.setTimeout(() => void refreshGenerationCapabilities(), 1_000);
  })();
}
window.addEventListener("focus", () => {
  void refreshHistory();
  void refreshGenerationCapabilities();
});
