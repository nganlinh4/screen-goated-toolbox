import "./styles.css";
import "../../ui-shared/creation-shell-layout.css";
import { setLanguage, t, type MessageKey } from "./i18n";
import { svgAppMarkup, SVG_ICONS } from "./layout";
import { SvgCanvasController } from "./svg-canvas";
import { SvgQueueView } from "./queue-view";
import type { Asset, HistoryEntry, HostContext, Item, JobStatus, Model, Stage } from "./types";

declare global {
  interface Window {
    invoke?: <T = unknown>(cmd: string, args?: unknown) => Promise<T>;
    __SGT_CONTEXT__?: HostContext;
    applyHostContext?: (context: HostContext) => void;
    handleNativeFileDrop?: (paths: string[]) => void;
    handleNativeFileDrag?: (active: boolean) => void;
  }
}

const hostContext = window.__SGT_CONTEXT__ || {};
const pageParams = new URLSearchParams(location.search);
const activeLanguage = pageParams.get("lang") || hostContext.language || "en";
setLanguage(activeLanguage);
document.documentElement.dataset.theme =
  (import.meta.env.DEV && pageParams.get("theme")) || hostContext.theme || "dark";

function invoke<T = unknown>(cmd: string, args: unknown = {}): Promise<T> {
  if (window.invoke) return window.invoke<T>(cmd, args);
  return Promise.reject(new Error("Desktop bridge unavailable"));
}

const app = document.querySelector<HTMLElement>("#app")!;
app.innerHTML = svgAppMarkup();
const query = <T extends Element>(selector: string) => document.querySelector<T>(selector)!;
const queueList = query<HTMLElement>("#queueList");
const folderPath = query<HTMLElement>("#folderPath");
const sourceThumb = query<HTMLElement>("#sourceThumb");
const statusTitle = query<HTMLElement>("#statusTitle");
const statusDetail = query<HTMLElement>("#statusDetail");
const statusEta = query<HTMLElement>("#statusEta");
const progressTrack = query<HTMLElement>("#progressTrack");
const progressFill = query<HTMLElement>("#progressFill");
const confirmDialog = query<HTMLElement>("#confirmDialog");
const confirmMessage = query<HTMLElement>("#confirmMessage");
const confirmCancel = query<HTMLButtonElement>("#confirmCancel");
const confirmAccept = query<HTMLButtonElement>("#confirmAccept");
const appToast = query<HTMLElement>("#appToast");

let items: Item[] = [];
let selectedId = "";
let outputDir = "";
let defaultModel: Model = "simple";
let pumping = false;
let historyRefreshing = false;
let confirmResolver: ((accepted: boolean) => void) | undefined;
let toastTimer = 0;

function basename(path: string) {
  return path.split(/[\\/]/).pop() || path;
}

function selected() {
  return items.find((item) => item.id === selectedId);
}

function busy(item: Item) {
  return ["preparing", "generating", "finalizing"].includes(item.stage);
}

function stageLabel(stage: Stage) {
  if (stage === "done") return t("done");
  if (stage === "failed") return t("failed");
  if (stage === "cancelled") return t("cancelled");
  if (stage === "draft") return t("selected");
  if (stage === "queued") return t("queued");
  return t("creating");
}

function confirmInApp(message: string) {
  confirmResolver?.(false);
  confirmMessage.textContent = message;
  confirmDialog.hidden = false;
  confirmAccept.focus();
  return new Promise<boolean>((resolve) => {
    confirmResolver = resolve;
  });
}

function closeConfirmation(accepted: boolean) {
  confirmDialog.hidden = true;
  const resolve = confirmResolver;
  confirmResolver = undefined;
  resolve?.(accepted);
}

function showToast(message: string) {
  window.clearTimeout(toastTimer);
  appToast.textContent = message;
  appToast.classList.add("visible");
  toastTimer = window.setTimeout(() => appToast.classList.remove("visible"), 4_200);
}

const progressKeys: Record<string, MessageKey> = {
  "svg.preparingWorkspace": "preparingWorkspace",
  "svg.confirmingWorkspace": "confirmingWorkspace",
  "svg.workspaceReady": "workspaceReady",
  "svg.openingWorkspace": "openingWorkspace",
  "svg.imageReady": "imageReady",
  "svg.creatingPaths": "creatingPaths",
  "svg.vectorReady": "finishingVector",
  "svg.waitingWorkspace": "waitingWorkspace",
  "svg.failed": "failedHint",
};

function localizedProgress(item: Item) {
  const key = item.progressKey && progressKeys[item.progressKey];
  if (key) return t(key);
  if (item.previewPath && busy(item)) return t("readingDepth");
  if (item.stage === "preparing") return t("preparingWorkspace");
  if (item.stage === "generating") return t("creatingPaths");
  if (item.stage === "finalizing") return t("finishingVector");
  if (item.stage === "failed") return t("failedHint");
  return item.outputName || "";
}

function fallbackEstimate(item: Item) {
  return item.model === "detail" ? 70_000 : 45_000;
}

function beginProgress(item: Item, status: JobStatus) {
  item.operationStartedAt = Date.now() - Math.max(0, status.elapsedMs || 0);
  item.estimatedTotalMs = status.estimatedTotalMs || fallbackEstimate(item);
  item.displayedProgress = Math.max(0, status.progressRatio || 0);
}

function formatRemaining(milliseconds: number) {
  if (milliseconds <= 15_000) return t("almostThere");
  if (milliseconds < 60_000) return t("lessMinute");
  return t("aboutMinutes", { count: Math.max(1, Math.ceil(milliseconds / 60_000)) });
}

function updateProgressUi() {
  const item = selected();
  const isBusy = Boolean(item && busy(item));
  progressTrack.classList.toggle("visible", isBusy);
  statusEta.classList.toggle("visible", isBusy);
  if (!item || !isBusy) {
    const done = item?.stage === "done";
    progressTrack.setAttribute("aria-valuenow", done ? "100" : "0");
    progressFill.style.width = done ? "100%" : "0%";
    statusEta.textContent = "";
    return;
  }
  const elapsedMs = Math.max(0, Date.now() - (item.operationStartedAt || Date.now()));
  const estimateMs = Math.max(10_000, item.estimatedTotalMs || fallbackEstimate(item));
  const curved = Math.min(0.94, 0.9 * (1 - Math.exp((-3 * elapsedMs) / estimateMs)));
  const reported = Math.max(0, Math.min(0.94, item.progress || 0));
  item.displayedProgress = Math.max(item.displayedProgress || 0, curved, reported);
  const percent = Math.round(item.displayedProgress * 100);
  progressTrack.setAttribute("aria-valuenow", String(percent));
  progressFill.style.width = `${percent}%`;
  statusEta.textContent =
    elapsedMs >= estimateMs ? t("takingLonger") : formatRemaining(estimateMs - elapsedMs);
}

async function loadSource(item: Item) {
  if (item.sourceUrl) return item.sourceUrl;
  const asset = await invoke<Asset>("read_image_preview", { path: item.path, maxEdge: 1_600 });
  item.sourceUrl = asset.dataUrl;
  return item.sourceUrl || "";
}

let canvas!: SvgCanvasController;
const queueView = new SvgQueueView({
  queueList,
  sourceThumb,
  getItems: () => items,
  getSelectedId: () => selectedId,
  stageLabel,
  invoke,
  onSelect: (item) => {
    selectedId = item.id;
    canvas.invalidate();
    render();
    void canvas.showItem(item);
  },
  onRename: (item, name) => void renameHistoryItem(item, name),
  onDelete: (item) => void deleteHistoryItem(item),
});
canvas = new SvgCanvasController({
  getSelected: selected,
  getItems: () => items,
  isSelected: (id) => selectedId === id,
  busy,
  loadSource,
  invoke,
  imageIcon: SVG_ICONS.image,
  vectorIcon: SVG_ICONS.vector,
  holdPreviews: (milliseconds) => queueView.holdPreviews(milliseconds),
  setPreviewInteraction: (active) => queueView.setInteractionActive(active),
});

function render() {
  queueView.render();
  const item = selected();
  query<HTMLButtonElement>("#generate").disabled = !item || item.stage !== "draft";
  query<HTMLButtonElement>("#cancel").hidden = !item || !busy(item);
  query<HTMLButtonElement>("#openFolder").hidden = !item?.outputPath;
  document.querySelectorAll<HTMLButtonElement>("[data-model]").forEach((button) => {
    const model = button.dataset.model as Model;
    button.classList.toggle("active", (item?.model || defaultModel) === model);
    button.disabled = Boolean(item && item.stage !== "draft");
  });
  if (item) {
    statusTitle.textContent = stageLabel(item.stage);
    statusDetail.textContent = localizedProgress(item);
    canvas.updateResultMeta(item);
  }
  canvas.syncEditorVisibility(item);
  updateProgressUi();
}

async function renameHistoryItem(item: Item, newName: string) {
  if (!item.historyId) return;
  try {
    const entry = await invoke<HistoryEntry>("rename_history_result", {
      id: item.historyId,
      newName,
    });
    item.outputPath = entry.outputPath;
    item.outputName = entry.outputName;
    if (item.id === selectedId) canvas.invalidate();
  } catch (error) {
    showToast(`${t("renameFailed")}: ${String(error)}`);
  } finally {
    queueView.finishRename();
    render();
  }
}

async function deleteHistoryItem(item: Item) {
  if (!item.historyId || !await confirmInApp(t("deleteResultConfirm"))) return;
  try {
    await invoke("delete_history_result", { id: item.historyId });
    const index = items.indexOf(item);
    items.splice(index, 1);
    if (selectedId === item.id) {
      canvas.clear();
      canvas.invalidate();
      selectedId = items[Math.min(index, items.length - 1)]?.id || "";
      if (!selectedId) canvas.showEmpty();
    }
    render();
    if (selected()) await canvas.showItem(selected());
  } catch (error) {
    showToast(`${t("deleteFailed")}: ${String(error)}`);
  }
}

function comparablePath(path?: string) {
  return (path || "").toLowerCase();
}

async function refreshHistory() {
  if (historyRefreshing || !window.invoke) return;
  historyRefreshing = true;
  try {
    const entries = await invoke<HistoryEntry[]>("history_results");
    const validIds = new Set(entries.map((entry) => entry.id));
    const validPaths = new Set(entries.map((entry) => comparablePath(entry.outputPath)));
    for (const entry of entries) {
      let item = items.find((candidate) => candidate.historyId === entry.id)
        || items.find((candidate) =>
          comparablePath(candidate.outputPath) === comparablePath(entry.outputPath));
      if (item) {
        item.historyId = entry.id;
        item.createdAtMs = entry.createdAtMs;
        item.outputPath = entry.outputPath;
        item.outputName = entry.outputName;
        continue;
      }
      const model = entry.metadata?.model === "detail" ? "detail" : "simple";
      item = {
        id: `history_${entry.id}`,
        batchId: `history_${entry.id}`,
        path: entry.sourcePath,
        name: basename(entry.sourcePath || entry.outputName),
        model,
        outputDir: entry.outputPath.replace(/[\\/][^\\/]+$/, ""),
        stage: "done",
        outputPath: entry.outputPath,
        outputName: entry.outputName,
        historyId: entry.id,
        createdAtMs: entry.createdAtMs,
      };
      items.push(item);
    }
    const selectedBefore = selectedId;
    items = items.filter((item) => {
      if (item.historyId) return validIds.has(item.historyId);
      return item.stage !== "done"
        || !item.outputPath
        || validPaths.has(comparablePath(item.outputPath));
    });
    if (!items.some((item) => item.id === selectedId)) selectedId = items[0]?.id || "";
    render();
    if (selectedId && selectedId !== selectedBefore) {
      canvas.invalidate();
      await canvas.showItem(selected());
    }
  } catch {
    // Keep the active queue available if history storage is unavailable.
  } finally {
    historyRefreshing = false;
  }
}

async function restoreCurrentJobs() {
  try {
    const statuses = await invoke<JobStatus[]>("job_statuses");
    for (const status of statuses.filter((value) =>
      ["preparing", "generating", "finalizing"].includes(value.stage))) {
      if (items.some((item) => item.jobId === status.jobId)) continue;
      items.push({
        id: `recovered_${status.jobId}`,
        batchId: `recovered_${status.jobId}`,
        path: status.sourceImagePath,
        name: basename(status.sourceImagePath),
        model: status.model,
        outputDir,
        stage: status.stage,
        jobId: status.jobId,
        progress: status.progressRatio,
        progressText: status.progressText,
        progressKey: status.progressKey,
        phase: status.phase,
        previewPath: status.previewPath,
        operationStartedAt: Date.now() - Math.max(0, status.elapsedMs || 0),
        estimatedTotalMs: status.estimatedTotalMs,
      });
    }
    if (!selectedId && items.length) selectedId = items[0].id;
  } catch {
    // There are no running jobs to recover.
  }
}

async function addImagePaths(paths: string[]) {
  if (!paths.length) return;
  const existing = new Set(items
    .filter((item) => item.stage === "draft" || item.stage === "queued" || busy(item))
    .map((item) => item.path.toLowerCase()));
  const uniquePaths = paths.filter((path) => {
    const key = path.toLowerCase();
    if (existing.has(key)) return false;
    existing.add(key);
    return true;
  });
  if (!uniquePaths.length) return;
  const batchId = `batch-${Date.now()}`;
  const created = uniquePaths.map((path, index): Item => ({
    id: `${batchId}-${index}`,
    batchId,
    path,
    name: basename(path),
    model: defaultModel,
    outputDir,
    stage: "draft",
  }));
  items.push(...created);
  selectedId = created[0].id;
  canvas.invalidate();
  render();
  void canvas.showItem(selected());
}

async function addImages() {
  await addImagePaths(await invoke<string[]>("pick_images"));
}

async function loadDepthFor(item: Item, path: string) {
  if (!path || item.previewPath === path && item.depthUrl) return;
  item.previewPath = path;
  try {
    item.depthUrl = (await invoke<Asset>("read_image_preview", {
      path,
      maxEdge: 1_024,
    })).dataUrl;
    if (item.id === selectedId && busy(item)) {
      canvas.invalidate();
      await canvas.showItem(item);
    }
  } catch {
    item.depthUrl = "";
  }
}

async function pump() {
  if (pumping) return;
  pumping = true;
  try {
    while (items.filter(busy).length < 2) {
      const item = items.find((value) => value.stage === "queued");
      if (!item) break;
      try {
        const status = await invoke<JobStatus>("start_job", {
          imagePath: item.path,
          outputDir: item.outputDir,
          model: item.model,
        });
        item.jobId = status.jobId;
        item.stage = status.stage;
        item.progressText = status.progressText;
        item.progressKey = status.progressKey;
        item.phase = status.phase;
        beginProgress(item, status);
        if (status.previewPath) void loadDepthFor(item, status.previewPath);
      } catch (error) {
        item.stage = "failed";
        item.error = error instanceof Error ? error.message : String(error);
      }
      render();
    }
  } finally {
    pumping = false;
  }
}

async function poll() {
  try {
    const statuses = await invoke<JobStatus[]>("job_statuses");
    for (const status of statuses) {
      const item = items.find((value) => value.jobId === status.jobId);
      if (!item) continue;
      const changed = item.stage !== status.stage;
      item.stage = status.stage;
      item.progress = status.progressRatio;
      item.progressText = status.progressText;
      item.outputPath = status.outputPath;
      item.outputName = status.outputName;
      item.error = status.error;
      item.progressKey = status.progressKey;
      item.phase = status.phase;
      if (busy(item)) {
        if (!item.operationStartedAt) beginProgress(item, status);
        if (status.estimatedTotalMs) item.estimatedTotalMs = status.estimatedTotalMs;
      }
      if (status.previewPath) void loadDepthFor(item, status.previewPath);
      if (changed && item.id === selectedId) {
        canvas.invalidate();
        void canvas.showItem(item);
      }
      if (changed && status.stage === "done") void refreshHistory();
    }
    render();
    void pump();
  } catch {
    // The host may be closing.
  }
}

query("#addImages").addEventListener("click", () => void addImages());
query("#chooseImages").addEventListener("click", () => void addImages());
query("#chooseFolder").addEventListener("click", async () => {
  const chosen = await invoke<string | null>("pick_output_dir");
  if (!chosen) return;
  outputDir = chosen;
  folderPath.textContent = chosen;
  const item = selected();
  if (item?.stage === "draft") {
    items
      .filter((value) => value.batchId === item.batchId && value.stage === "draft")
      .forEach((value) => value.outputDir = chosen);
  }
});
query("#generate").addEventListener("click", () => {
  const item = selected();
  if (!item || item.stage !== "draft") return;
  item.stage = "queued";
  render();
  void pump();
});
query("#cancel").addEventListener("click", async () => {
  const item = selected();
  if (item?.jobId) await invoke("cancel_job", { jobId: item.jobId });
  void poll();
});
query("#openFolder").addEventListener("click", () => {
  const item = selected();
  void invoke("open_output", { path: item?.outputPath || outputDir });
});
document.querySelectorAll<HTMLButtonElement>("[data-model]").forEach((button) => {
  button.addEventListener("click", () => {
    const model = button.dataset.model as Model;
    defaultModel = model;
    const item = selected();
    if (item?.stage === "draft") {
      items
        .filter((value) => value.batchId === item.batchId && value.stage === "draft")
        .forEach((value) => value.model = model);
    }
    render();
  });
});
query("#minimize").addEventListener("click", () => void invoke("minimize_window"));
query("#close").addEventListener("click", () => void invoke("close_window"));
query("#dragRegion").addEventListener("mousedown", (event) => {
  if (!(event.target as Element).closest("button")) void invoke("start_drag");
});
confirmCancel.addEventListener("click", () => closeConfirmation(false));
confirmAccept.addEventListener("click", () => closeConfirmation(true));
confirmDialog.addEventListener("click", (event) => {
  if (event.target === confirmDialog) closeConfirmation(false);
});

window.handleNativeFileDrag = (active) => document.body.classList.toggle("file-dragging", active);
window.handleNativeFileDrop = (paths) => {
  document.body.classList.remove("file-dragging");
  void addImagePaths(paths);
};
window.applyHostContext = (next) => {
  if (next.theme) document.documentElement.dataset.theme = next.theme;
  if (next.language && next.language !== activeLanguage) {
    const url = new URL(location.href);
    url.searchParams.set("lang", next.language);
    location.replace(url.toString());
  }
};

async function boot() {
  outputDir = await invoke<string>("default_output_dir").catch(() => "Downloads");
  folderPath.textContent = outputDir;
  const demoMode = import.meta.env.DEV && pageParams.has("demo");
  if (demoMode) {
    const svg = `<svg viewBox="0 0 640 480" xmlns="http://www.w3.org/2000/svg"><path fill="#edf2ff" d="M70 60h500v360H70z"/><path fill="#315fce" d="M112 120h182v96H112z"/><path fill="#ff7b6b" d="M330 120h198v44H330z"/><path fill="#55cda7" d="M330 184h140v32H330z"/><path fill="#252c39" d="M112 252h416v24H112z"/><path fill="#8da8ef" d="M112 300h310v20H112z"/><path fill="#cad4e7" d="M112 340h370v20H112z"/></svg>`;
    const demo: Item = {
      id: "demo",
      batchId: "demo",
      path: "sample.png",
      name: "sample.png",
      model: "simple",
      outputDir,
      stage: "done",
      outputPath: "demo.svg",
      outputName: "sample.svg",
      svgText: svg,
      historyId: pageParams.has("history") ? "demo-history" : undefined,
    };
    items = [demo];
    selectedId = demo.id;
  } else if (window.invoke) {
    await restoreCurrentJobs();
    await refreshHistory();
    window.setInterval(() => void refreshHistory(), 5_000);
  }
  const updateReady = async () => {
    const status = await invoke<string>("runtime_preparation_status").catch(() => "preparing");
    const badge = query<HTMLElement>("#readiness");
    badge.className = `readiness ${status === "ready" ? "" : "busy"}`;
    query("#readyText").textContent =
      status === "ready" ? t("ready") : status === "partial" ? t("oneWorker") : t("preparing");
  };
  void invoke("prepare_runtime");
  void updateReady();
  window.setInterval(updateReady, 2_500);
  window.setInterval(poll, 700);
  window.setInterval(updateProgressUi, 250);
  render();
  await canvas.showItem(selected());
  if (demoMode && selected()) {
    const demoZoom = Number(pageParams.get("zoom"));
    if (Number.isFinite(demoZoom) && demoZoom > 0) canvas.setZoom(demoZoom);
    if (pageParams.has("selected")) canvas.selectFirstShape();
  }
}

void boot();
window.addEventListener("focus", () => void refreshHistory());
