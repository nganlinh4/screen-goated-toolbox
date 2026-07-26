import "./styles.css";
import "../../ui-shared/creation-shell-layout.css";
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
  "preparing",
  "visualizing",
  "generating",
  "segmenting",
  "finalizing",
]);
const MAX_PARALLEL_JOBS = 2;
const initialContext = window.__SGT_CONTEXT__ || {};
const devParams = import.meta.env.DEV ? new URLSearchParams(window.location.search) : null;
setLocale(devParams?.get("lang") || initialContext.language);
document.documentElement.dataset.theme =
  devParams?.get("theme")
  || initialContext.theme
  || document.documentElement.dataset.theme
  || "dark";

function invoke<T = unknown>(cmd: string, args: unknown = {}): Promise<T> {
  if (window.invoke) return window.invoke<T>(cmd, args);
  return Promise.reject(new Error("The desktop bridge is not available."));
}

const app = document.querySelector<HTMLElement>("#app");
if (!app) throw new Error("App root not found");
app.innerHTML = appMarkup();
const nodes = collectNodes();
const viewer = new ModelViewer(nodes.canvas, nodes.stage);
const state: AppState = {
  items: [],
  selectedId: "",
  runningIds: new Set<string>(),
  outputDir: "",
  queueActive: false,
  cancelRequested: false,
  backendStatus: { stage: "idle", progressText: "", runtimeStatus: "checking" },
  preparationStatus: "preparing",
  preparationTimer: 0,
  preparationPollToken: 0,
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
};

function pathLeaf(path: string) {
  return path.split(/[\\/]/).filter(Boolean).pop() || path;
}

function stripExtension(name: string) {
  return name.replace(/\.[^.]+$/, "");
}

function selectedItem() {
  return state.items.find((item) => item.id === state.selectedId);
}

function pendingItems() {
  return state.items.filter((item) => item.state === "queued" && item.submitted);
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
  return Boolean(item && ["done", "failed", "cancelled"].includes(item.state));
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

let confirmResolver: ((accepted: boolean) => void) | undefined;
let toastTimer = 0;

function confirmInApp(message: string) {
  confirmResolver?.(false);
  nodes.confirmMessage.textContent = message;
  nodes.confirmDialog.hidden = false;
  nodes.confirmAccept.focus();
  return new Promise<boolean>((resolve) => {
    confirmResolver = resolve;
  });
}

function closeConfirmation(accepted: boolean) {
  nodes.confirmDialog.hidden = true;
  const resolve = confirmResolver;
  confirmResolver = undefined;
  resolve?.(accepted);
}

function showToast(message: string) {
  window.clearTimeout(toastTimer);
  nodes.appToast.textContent = message;
  nodes.appToast.classList.add("visible");
  toastTimer = window.setTimeout(() => nodes.appToast.classList.remove("visible"), 4_200);
}

function closeReferencePreview() {
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
    const previewUrl = (await readImagePreview(item.path, 1_600)).dataUrl;
    if (token !== state.referencePreviewToken || state.referencePreviewItemId !== item.id) return;
    nodes.referencePreviewImage.src = previewUrl;
  } catch {
    if (token !== state.referencePreviewToken) return;
    closeReferencePreview();
    showToast(t("referenceUnavailable"));
  }
}

async function readAsset(path: string) {
  return invoke<AssetPayload>("read_asset", { path });
}

async function readImagePreview(path: string, maxEdge: number) {
  return invoke<AssetPayload>("read_image_preview", { path, maxEdge });
}

function readModelAsset(item: QueueItem, path: string) {
  if (item.modelAssetPath !== path || !item.modelAssetPromise) {
    item.modelAssetPath = path;
    const promise = readAsset(path);
    item.modelAssetPromise = promise;
    const clear = () => {
      if (item.modelAssetPromise === promise) {
        item.modelAssetPromise = undefined;
        item.modelAssetPath = undefined;
      }
    };
    void promise.then(clear, clear);
  }
  return item.modelAssetPromise;
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
  readImagePreview,
  onSelect: (id) => void selectItem(id),
  onOpenReference: (item) => void openReferencePreview(item),
  onRemove: removeItem,
  onRename: (item, name) => void renameHistoryItem(item, name),
  onDelete: (item) => void deleteHistoryItem(item),
});
viewer.onInteractionChange((active) => queueView.setInteractionActive(active, 220));
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
  maxParallelJobs: MAX_PARALLEL_JOBS,
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
    }
  } catch (error) {
    showToast(`${t("renameFailed")}: ${String(error)}`);
  } finally {
    queueView.finishRename();
    updateUi();
  }
}

async function deleteHistoryItem(item: QueueItem) {
  if (!item.historyId || !await confirmInApp(t("deleteResultConfirm"))) return;
  try {
    await invoke("delete_history_result", { id: item.historyId });
    removeItem(item.id);
  } catch (error) {
    showToast(`${t("deleteFailed")}: ${String(error)}`);
  }
}

async function refreshHistory() {
  if (state.historyRefreshing || !window.invoke) return;
  state.historyRefreshing = true;
  try {
    const entries = await invoke<HistoryEntry[]>("history_results");
    const validIds = new Set(entries.map((entry) => entry.id));
    for (const entry of entries) {
      let item = state.items.find((candidate) => candidate.historyId === entry.id)
        || state.items.find((candidate) =>
          comparablePath(candidate.result?.outputPath) === comparablePath(entry.outputPath));
      if (item) {
        item.historyId = entry.id;
        item.createdAtMs = entry.createdAtMs;
        if (item.result) {
          item.result.outputPath = entry.outputPath;
          item.result.outputName = entry.outputName;
          item.result.isSegmented = Boolean(entry.metadata?.isSegmented);
        }
        continue;
      }
      const sourcePath = entry.sourcePath;
      const name = pathLeaf(sourcePath || entry.outputName);
      state.items.push({
        id: `history_${entry.id}`,
        batchId: `history_${entry.id}`,
        path: sourcePath,
        name,
        extension: name.split(".").pop()?.toUpperCase() || t("image"),
        generationMode: entry.metadata?.generationMode || "quality",
        polycount: 5_000,
        autoSegment: Boolean(entry.metadata?.isSegmented),
        submitted: true,
        state: "done",
        historyId: entry.id,
        createdAtMs: entry.createdAtMs,
        result: {
          stage: "done",
          progressText: "",
          outputPath: entry.outputPath,
          outputName: entry.outputName,
          sourceImagePath: sourcePath,
          isSegmented: Boolean(entry.metadata?.isSegmented),
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
  const existing = new Set(state.items
    .filter((item) => item.state === "queued" || item.state === "running")
    .map((item) => item.path.toLowerCase()));
  const unique = paths.filter((path) => {
    const key = path.toLowerCase();
    if (existing.has(key)) return false;
    existing.add(key);
    return true;
  });
  if (!unique.length) return;
  const batchId = `batch_${Date.now()}_${Math.random().toString(36).slice(2)}`;
  const created = unique.map((path): QueueItem => {
    const name = pathLeaf(path);
    return {
      id: `image_${Date.now()}_${Math.random().toString(36).slice(2)}`,
      batchId,
      path,
      name,
      extension: name.split(".").pop()?.toUpperCase() || t("image"),
      generationMode: "quality",
      polycount: 5_000,
      autoSegment: false,
      submitted: false,
      state: "queued",
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
      if (modelPath) {
        const asset = await readModelAsset(item, modelPath);
        if (token !== state.displayToken || state.selectedId !== item.id) return;
        const stats = await viewer.setModel(asset.dataUrl, Boolean(item.result?.isSegmented));
        if (!stats) return;
        item.modelStats = stats;
        item.loadedModelPath = modelPath;
        if (token !== state.displayToken || state.selectedId !== item.id) return;
        state.displayedItemId = item.id;
        state.displayedModelPath = modelPath;
        updateUi();
        return;
      }
      const sourcePreview = (await readImagePreview(item.path, 1_600)).dataUrl;
      if (token !== state.displayToken || state.selectedId !== item.id) return;
      await viewer.setSource(sourcePreview);
      if (token !== state.displayToken || state.selectedId !== item.id) return;
      state.displayedItemId = item.id;
      state.displayedModelPath = "";
      item.loadedModelPath = "";
      item.loadedDepthPath = "";
      if (item.result?.previewPath) await loadDepthFor(item, item.result.previewPath);
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

async function loadDepthFor(item: QueueItem, path: string) {
  if (!path || item.loadedDepthPath === path || state.selectedId !== item.id) return;
  item.loadedDepthPath = path;
  try {
    await viewer.setDepth((await readImagePreview(path, 1_024)).dataUrl);
  } catch {
    item.loadedDepthPath = "";
  }
}

jobRunner = new JobRunner({
  state,
  busyStages: BUSY_STAGES,
  maxParallelJobs: MAX_PARALLEL_JOBS,
  invoke,
  normalizeSettings: normalizeGenerationSettings,
  selectedItem,
  pendingItems,
  activeJobCount,
  pathLeaf,
  displayItem,
  loadDepthFor,
  refreshHistory,
  updateUi,
  beginProgress: (item, estimateMs) => presentation.beginProgress(item, estimateMs),
});

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
  closeConfirmation,
  closeReferencePreview,
});

async function loadDefaultOutputDir() {
  try {
    state.outputDir = await invoke<string>("default_output_dir");
    updateUi();
  } catch {
    // Browser-only previews have no native output directory.
  }
}

presentation.applyTranslations();
updateUi();
window.setInterval(() => presentation.updateProgressUi(), 250);
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
  void invoke("prepare_runtime")
    .catch(() => undefined)
    .finally(() => jobRunner.startPreparationPolling());
  void (async () => {
    await loadDefaultOutputDir();
    await jobRunner.restoreCurrentJobs();
    await refreshHistory();
    window.setInterval(() => void refreshHistory(), 5_000);
  })();
}
window.addEventListener("focus", () => void refreshHistory());
