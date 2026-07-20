import "./styles.css";
import { locale, setLocale, t, type MessageKey } from "./i18n";
import { ModelViewer, type ModelStats, type ShadingMode } from "./viewer";

type Stage = "idle" | "runtime_missing" | "preparing" | "visualizing" | "generating" | "segmenting" | "finalizing" | "done" | "failed" | "cancelled";
type QueueState = "queued" | "running" | "done" | "failed" | "cancelled";

type JobStatus = {
  jobId?: string | null;
  stage: Stage;
  progressText: string;
  phase?: string | null;
  workspaceState?: string | null;
  elapsedMs?: number | null;
  estimatedTotalMs?: number | null;
  progressRatio?: number | null;
  timingSampleCount?: number | null;
  outputPath?: string | null;
  outputName?: string | null;
  previewPath?: string | null;
  sourceImagePath?: string | null;
  isSegmented?: boolean;
  canSegment?: boolean;
  error?: string | null;
  runtimeStatus?: string;
};

type StartJobRequest = {
  imagePath: string;
  outputDir?: string | null;
  polycount: number;
  mode: "topology_mesh";
  outputFormat: "glb_plain";
  autoSegment: boolean;
  segmentationMode: "parts" | "none";
};

type AssetPayload = { dataUrl: string; sizeBytes: number };
type HostContext = { theme?: "light" | "dark"; language?: string };

type QueueItem = {
  id: string;
  batchId: string;
  path: string;
  name: string;
  extension: string;
  assetUrl: string;
  polycount: number;
  autoSegment: boolean;
  submitted: boolean;
  state: QueueState;
  result?: JobStatus;
  loadedDepthPath?: string;
  operationStartedAt?: number;
  estimatedTotalMs?: number;
  displayedProgress?: number;
  modelStats?: ModelStats;
};

declare global {
  interface Window {
    invoke?: <T = unknown>(cmd: string, args?: unknown) => Promise<T>;
    ipc?: { postMessage: (message: string) => void };
    __SGT_CONTEXT__?: HostContext;
    __SGT_PARALLEL_TEST__?: { starts: string[]; active: number; maxActive: number; completed: number };
    applyHostContext?: (context: HostContext) => void;
  }
}

const BUSY_STAGES = new Set<Stage>(["preparing", "visualizing", "generating", "segmenting", "finalizing"]);
const initialContext = window.__SGT_CONTEXT__ || {};
const devParams = import.meta.env.DEV ? new URLSearchParams(window.location.search) : null;
setLocale(devParams?.get("lang") || initialContext.language);
document.documentElement.dataset.theme = devParams?.get("theme") || initialContext.theme || document.documentElement.dataset.theme || "dark";

function invoke<T = unknown>(cmd: string, args: unknown = {}): Promise<T> {
  if (window.invoke) return window.invoke<T>(cmd, args);
  return Promise.reject(new Error("The desktop bridge is not available."));
}

function icon(path: string, viewBox = "0 -960 960 960") {
  return `<svg aria-hidden="true" viewBox="${viewBox}" focusable="false"><path d="${path}"/></svg>`;
}

const ICONS = {
  model: icon("M480-80 120-280v-400l360-200 360 200v400L480-80Zm0-92 280-155v-286L480-458 200-613v286l280 155Zm0-378 274-152-274-152-274 152 274 152Z"),
  image: icon("M200-120q-33 0-56.5-23.5T120-200v-560q0-33 23.5-56.5T200-840h560q33 0 56.5 23.5T840-760v560q0 33-23.5 56.5T760-120H200Zm80-160h400q12 0 18-11t-2-21L586-459q-6-8-16-8t-16 8L450-320l-74-99q-6-8-16-8t-16 8l-80 107q-8 10-2 21t18 11Z"),
  folder: icon("M160-160q-33 0-56.5-23.5T80-240v-480q0-33 23.5-56.5T160-800h207q16 0 30.5 6t25.5 17l57 57h360q17 0 28.5 11.5T880-680q0 17-11.5 28.5T840-640H314q-62 0-108 39t-46 99v262l79-263q8-26 29.5-41.5T316-560h516q41 0 64.5 32.5T909-457l-72 240q-8 26-29.5 41.5T760-160H160Z"),
  sparkle: icon("M706-706l-70-32q-11-5-11-18t11-18l70-32 32-70q5-12 18-12t18 12l32 70 70 32q12 5 12 18t-12 18l-70 32-32 70q-5 11-18 11t-18-11l-32-70ZM260-380l-160-73q-17-8-17-27t17-27l160-73 73-160q8-17 27-17t27 17l73 160 160 73q17 8 17 27t-17 27l-160 73-73 160q-8 17-27 17t-27-17l-73-160Zm450 230-70-32q-12-5-12-18t12-18l70-32 32-70q5-12 18-12t18 12l32 70 70 32q12 5 12 18t-12 18l-70 32-32 70q-5 12-18 12t-18-12l-32-70Z"),
  stop: icon("M280-240q-33 0-56.5-23.5T200-320v-320q0-33 23.5-56.5T280-720h400q33 0 56.5 23.5T760-640v320q0 33-23.5 56.5T680-240H280Z"),
  close: icon("M480-424 284-228q-11 11-28 11t-28-11q-11-11-11-28t11-28l196-196-196-196q-11-11-11-28t11-28q11-11 28-11t28 11l196 196 196-196q11-11 28-11t28 11q11 11 11 28t-11 28L536-480l196 196q11 11 11 28t-11 28q-11 11-28 11t-28-11L480-424Z"),
  minimize: icon("M240-440q-17 0-28.5-11.5T200-480q0-17 11.5-28.5T240-520h480q17 0 28.5 11.5T760-480q0 17-11.5 28.5T720-440H240Z"),
  check: icon("m424-408-86-86q-11-11-28-11t-28 11q-11 11-11 28t11 28l114 114q12 12 28 12t28-12l226-226q11-11 11-28t-11-28q-11-11-28-11t-28 11L424-408Z"),
  add: icon("M440-440H240q-17 0-28.5-11.5T200-480q0-17 11.5-28.5T240-520h200v-200q0-17 11.5-28.5T480-760q17 0 28.5 11.5T520-720v200h200q17 0 28.5 11.5T760-480q0 17-11.5 28.5T720-440H520v200q0 17-11.5 28.5T480-200q-17 0-28.5-11.5T440-240v-200Z"),
  palette: icon("M480-80q-83 0-156-31.5T197-197q-54-54-85.5-127T80-480q0-83 32.5-156t88-127Q256-817 331-848.5T488-880q80 0 151 27.5t124.5 76Q817-728 848.5-661T880-520q0 58-35 93t-93 35h-68q-12 0-22 4t-18 12q-8 8-12 18t-4 22q0 27 22 50.5t22 56.5q0 55-61 102T480-80Z"),
  toon: icon("M200-120q-33 0-56.5-23.5T120-200v-560q0-33 23.5-56.5T200-840h560q33 0 56.5 23.5T840-760v560q0 33-23.5 56.5T760-120H200Zm40-160h480v-160H240v160Zm0-240h480v-160H240v160Z"),
  outline: icon("M200-120q-33 0-56.5-23.5T120-200v-560q0-33 23.5-56.5T200-840h560q33 0 56.5 23.5T840-760v560q0 33-23.5 56.5T760-120H200Zm0-80h560v-560H200v560Zm80-80v-400h400v400H280Z"),
  rotate: icon("M480-160q-134 0-227-93t-93-227q0-134 93-227t227-93q84 0 157 39t119 105v-104q0-17 11.5-28.5T796-800q17 0 28.5 11.5T836-760v240H596q-17 0-28.5-11.5T556-560q0-17 11.5-28.5T596-600h106q-34-55-92-87.5T480-720q-100 0-170 70t-70 170q0 100 70 170t170 70q68 0 125-35t91-93q9-15 25.5-20t31.5 4q15 9 19.5 25t-4.5 31q-46 75-122.5 121.5T480-160Z"),
  grid: icon("M120-120v-720h720v720H120Zm80-480h160v-160H200v160Zm240 0h160v-160H440v160Zm240 0h80v-160h-80v160ZM200-360h160v-160H200v160Zm240 0h160v-160H440v160Zm240 0h80v-160h-80v160ZM200-200h160v-80H200v80Zm240 0h160v-80H440v80Zm240 0h80v-80h-80v80Z"),
  wire: icon("M160-120q-17 0-28.5-11.5T120-160v-640q0-17 11.5-28.5T160-840h640q17 0 28.5 11.5T840-800v640q0 17-11.5 28.5T800-120H160Zm40-80h240v-240H200v240Zm320 0h240v-240H520v240ZM200-520h240v-240H200v240Zm320 0h240v-240H520v240Z"),
  fit: icon("M200-120q-33 0-56.5-23.5T120-200v-160q0-17 11.5-28.5T160-400q17 0 28.5 11.5T200-360v160h160q17 0 28.5 11.5T400-160q0 17-11.5 28.5T360-120H200Zm560 0H600q-17 0-28.5-11.5T560-160q0-17 11.5-28.5T600-200h160v-160q0-17 11.5-28.5T800-400q17 0 28.5 11.5T840-360v160q0 33-23.5 56.5T760-120ZM160-560q-17 0-28.5-11.5T120-600v-160q0-33 23.5-56.5T200-840h160q17 0 28.5 11.5T400-800q0 17-11.5 28.5T360-760H200v160q0 17-11.5 28.5T160-560Zm640 0q-17 0-28.5-11.5T760-600v-160H600q-17 0-28.5-11.5T560-800q0-17 11.5-28.5T600-840h160q33 0 56.5 23.5T840-760v160q0 17-11.5 28.5T800-560Z"),
};

const app = document.querySelector<HTMLElement>("#app");
if (!app) throw new Error("App root not found");

app.innerHTML = `
  <section class="app-shell">
    <header class="titlebar" id="dragRegion">
      <div class="identity">
        <span class="app-icon">${ICONS.model}</span>
        <strong data-i18n="appTitle"></strong>
        <span class="readiness" id="readiness" data-i18n-title="readyTooltip"><i></i><span id="readinessText"></span></span>
      </div>
      <div class="window-actions">
        <button class="icon-button" id="minimizeButton" type="button" data-i18n-title="minimize">${ICONS.minimize}</button>
        <button class="icon-button close" id="closeButton" type="button" data-i18n-title="close">${ICONS.close}</button>
      </div>
    </header>

    <main class="workspace">
      <aside class="queue-rail">
        <div class="queue-header">
          <span class="control-label" data-i18n="queue"></span>
          <button class="icon-button add-button" id="addImagesButton" type="button" data-i18n-title="addImages">${ICONS.add}</button>
        </div>
        <div class="queue-list" id="queueList"></div>
        <div class="queue-footer" id="queueFooter"></div>
      </aside>

      <section class="model-stage" id="modelStage">
        <canvas id="modelCanvas" data-i18n-aria="preview"></canvas>
        <div class="empty-copy" id="emptyCopy">
          <strong data-i18n="emptyTitle"></strong>
          <span data-i18n="emptyDetail"></span>
        </div>
        <div class="viewer-toolbar" id="viewerToolbar">
          <span class="tool-segment" role="group">
            <button class="view-tool shading-tool" type="button" data-shading="original" data-i18n-title="originalMaterials">${ICONS.model}</button>
            <button class="view-tool shading-tool" type="button" data-shading="toon" data-i18n-title="toonOutline">${ICONS.toon}</button>
            <button class="view-tool shading-tool" type="button" data-shading="parts" data-i18n-title="partColors">${ICONS.palette}</button>
          </span>
          <span class="tool-divider"></span>
          <button class="view-tool active" id="outlineButton" type="button" data-i18n-title="toggleOutline">${ICONS.outline}</button>
          <button class="view-tool" id="rotateButton" type="button" data-i18n-title="toggleRotation">${ICONS.rotate}</button>
          <button class="view-tool" id="gridButton" type="button" data-i18n-title="toggleGrid">${ICONS.grid}</button>
          <button class="view-tool" id="wireButton" type="button" data-i18n-title="toggleWireframe">${ICONS.wire}</button>
          <button class="view-tool" id="fitButton" type="button" data-i18n-title="resetView">${ICONS.fit}</button>
        </div>
        <div class="stage-status" id="stageStatus" aria-live="polite">
          <span class="status-mark" id="statusMark">${ICONS.sparkle}</span>
          <span class="status-copy">
            <span class="status-heading"><strong id="statusTitle"></strong><small class="status-eta" id="statusEta"></small></span>
            <small id="statusDetail"></small>
            <span class="progress-track" id="progressTrack" role="progressbar" aria-valuemin="0" aria-valuemax="100"><i id="progressFill"></i></span>
          </span>
        </div>
        <div class="model-stats" id="modelStats"></div>
        <button class="floating-action" id="showFolderButton" type="button" data-i18n-title="showInFolder">${ICONS.folder}</button>
      </section>

      <aside class="control-rail">
        <div class="control-section source-section">
          <span class="control-label" data-i18n="image"></span>
          <button class="source-button" id="chooseImageButton" type="button">
            <span class="source-thumb" id="sourceThumb">${ICONS.image}</span>
            <span class="source-copy"><strong id="sourceName"></strong><small id="sourceMeta"></small></span>
          </button>
        </div>
        <div class="control-section">
          <div class="control-heading"><label for="polycountRange" data-i18n="topology"></label><output id="polycountValue">5,000</output></div>
          <input class="range" id="polycountRange" type="range" min="500" max="20000" step="100" value="5000" />
          <div class="range-scale"><span data-i18n="light"></span><span data-i18n="detailed"></span></div>
        </div>
        <div class="control-section compact">
          <button class="folder-row" id="chooseFolderButton" type="button">
            <span>${ICONS.folder}</span><span><small data-i18n="saveTo"></small><strong id="folderName"></strong></span>
          </button>
        </div>
        <div class="control-section compact">
          <label class="switch-row" for="autoSegmentInput">
            <span><strong data-i18n="autoSeparateParts"></strong><small data-i18n="colorReadyPieces"></small></span>
            <input id="autoSegmentInput" type="checkbox" /><i class="switch" aria-hidden="true"></i>
          </label>
        </div>
        <div class="rail-spacer"></div>
        <div class="result-summary" id="resultSummary">
          <span>${ICONS.check}</span><span><strong id="resultName"></strong><small id="resultMeta"></small></span>
        </div>
        <button class="secondary-action" id="segmentButton" type="button" data-i18n="separateParts"></button>
        <button class="primary-action" id="generateButton" type="button" disabled><span>${ICONS.sparkle}</span><span id="generateLabel"></span></button>
        <button class="cancel-action" id="cancelButton" type="button"><span>${ICONS.stop}</span><span data-i18n="cancel"></span></button>
      </aside>
    </main>
  </section>
`;

const query = <T extends Element>(selector: string) => document.querySelector<T>(selector)!;
const nodes = {
  dragRegion: query<HTMLElement>("#dragRegion"), minimizeButton: query<HTMLButtonElement>("#minimizeButton"), closeButton: query<HTMLButtonElement>("#closeButton"),
  addImagesButton: query<HTMLButtonElement>("#addImagesButton"), queueList: query<HTMLElement>("#queueList"), queueFooter: query<HTMLElement>("#queueFooter"),
  chooseImageButton: query<HTMLButtonElement>("#chooseImageButton"), chooseFolderButton: query<HTMLButtonElement>("#chooseFolderButton"),
  showFolderButton: query<HTMLButtonElement>("#showFolderButton"), sourceThumb: query<HTMLElement>("#sourceThumb"), sourceName: query<HTMLElement>("#sourceName"),
  sourceMeta: query<HTMLElement>("#sourceMeta"), folderName: query<HTMLElement>("#folderName"), polycountRange: query<HTMLInputElement>("#polycountRange"),
  polycountValue: query<HTMLOutputElement>("#polycountValue"), autoSegmentInput: query<HTMLInputElement>("#autoSegmentInput"),
  generateButton: query<HTMLButtonElement>("#generateButton"), generateLabel: query<HTMLElement>("#generateLabel"), cancelButton: query<HTMLButtonElement>("#cancelButton"),
  segmentButton: query<HTMLButtonElement>("#segmentButton"), statusTitle: query<HTMLElement>("#statusTitle"), statusDetail: query<HTMLElement>("#statusDetail"),
  statusEta: query<HTMLElement>("#statusEta"), progressTrack: query<HTMLElement>("#progressTrack"), progressFill: query<HTMLElement>("#progressFill"),
  statusMark: query<HTMLElement>("#statusMark"), stageStatus: query<HTMLElement>("#stageStatus"), readiness: query<HTMLElement>("#readiness"),
  readinessText: query<HTMLElement>("#readinessText"), emptyCopy: query<HTMLElement>("#emptyCopy"), modelStats: query<HTMLElement>("#modelStats"),
  resultSummary: query<HTMLElement>("#resultSummary"),
  resultName: query<HTMLElement>("#resultName"), resultMeta: query<HTMLElement>("#resultMeta"), canvas: query<HTMLCanvasElement>("#modelCanvas"),
  stage: query<HTMLElement>("#modelStage"), viewerToolbar: query<HTMLElement>("#viewerToolbar"), outlineButton: query<HTMLButtonElement>("#outlineButton"),
  rotateButton: query<HTMLButtonElement>("#rotateButton"), gridButton: query<HTMLButtonElement>("#gridButton"), wireButton: query<HTMLButtonElement>("#wireButton"),
  fitButton: query<HTMLButtonElement>("#fitButton"), shadingButtons: [...document.querySelectorAll<HTMLButtonElement>(".shading-tool")],
};

const viewer = new ModelViewer(nodes.canvas, nodes.stage);
const MAX_PARALLEL_JOBS = 2;
const state = {
  items: [] as QueueItem[], selectedId: "", runningIds: new Set<string>(), outputDir: "", queueActive: false, cancelRequested: false,
  backendStatus: { stage: "idle", progressText: "", runtimeStatus: "checking" } as JobStatus,
  preparationStatus: "preparing", preparationTimer: 0, preparationPollToken: 0, displayToken: 0,
  outline: true, rotate: false, grid: false, wire: false,
};

function pathLeaf(path: string) { return path.split(/[\\/]/).filter(Boolean).pop() || path; }
function stripExtension(name: string) { return name.replace(/\.[^.]+$/, ""); }
function selectedItem() { return state.items.find((item) => item.id === state.selectedId); }
function pendingItems() { return state.items.filter((item) => item.state === "queued" && item.submitted); }
function batchItems(batchId: string) { return state.items.filter((item) => item.batchId === batchId); }
function activeJobCount() { return state.runningIds.size; }
function delay(ms: number) { return new Promise((resolve) => window.setTimeout(resolve, ms)); }

function applyTranslations() {
  document.querySelectorAll<HTMLElement>("[data-i18n]").forEach((node) => { node.textContent = t(node.dataset.i18n as MessageKey); });
  document.querySelectorAll<HTMLElement>("[data-i18n-title]").forEach((node) => {
    const value = t(node.dataset.i18nTitle as MessageKey); node.title = value; node.setAttribute("aria-label", value);
  });
  document.querySelectorAll<HTMLElement>("[data-i18n-aria]").forEach((node) => node.setAttribute("aria-label", t(node.dataset.i18nAria as MessageKey)));
}

function queueStateLabel(value: QueueState) {
  return t(value === "running" ? "creating" : value === "done" ? "complete" : value === "failed" ? "failed" : "queued");
}

function itemQueueLabel(item: QueueItem) {
  return item.state === "queued" && !item.submitted ? t("draft") : queueStateLabel(item.state);
}

function renderQueue() {
  nodes.queueList.replaceChildren();
  if (!state.items.length) {
    const empty = document.createElement("div");
    empty.className = "queue-empty";
    empty.innerHTML = `<span>${ICONS.image}</span><strong>${t("queueEmpty")}</strong><small>${t("queueEmptyDetail")}</small>`;
    nodes.queueList.append(empty);
  }
  const batchIds = [...new Set(state.items.map((item) => item.batchId))];
  const showBatchLabels = batchIds.length > 1 || state.items.some((item) => batchItems(item.batchId).length > 1);
  let previousBatchId = "";
  for (const item of state.items) {
    if (showBatchLabels && item.batchId !== previousBatchId) {
      const batchHeader = document.createElement("div");
      batchHeader.className = "batch-label";
      batchHeader.textContent = t("batchLabel", {
        number: batchIds.indexOf(item.batchId) + 1,
        count: batchItems(item.batchId).length,
      });
      nodes.queueList.append(batchHeader);
      previousBatchId = item.batchId;
    }
    const row = document.createElement("div");
    row.className = `queue-item ${item.id === state.selectedId ? "selected" : ""}`;
    row.dataset.state = item.state;
    const button = document.createElement("button");
    button.type = "button";
    button.className = "queue-item-main";
    const thumb = document.createElement("span");
    thumb.className = "queue-thumb";
    thumb.innerHTML = item.assetUrl ? `<img alt="" src="${item.assetUrl}">` : ICONS.image;
    const copy = document.createElement("span");
    copy.className = "queue-copy";
    const strong = document.createElement("strong"); strong.textContent = stripExtension(item.name);
    const small = document.createElement("small"); small.textContent = itemQueueLabel(item);
    copy.append(strong, small); button.append(thumb, copy);
    button.addEventListener("click", () => void selectItem(item.id));
    const remove = document.createElement("button");
    remove.type = "button"; remove.className = "queue-remove"; remove.innerHTML = ICONS.close;
    remove.title = t("remove"); remove.setAttribute("aria-label", t("remove")); remove.disabled = item.state === "running";
    remove.addEventListener("click", () => removeItem(item.id));
    row.append(button, remove); nodes.queueList.append(row);
  }
  nodes.queueFooter.textContent = state.items.length ? t("jobsCount", { count: state.items.length }) : "";
}

async function readAsset(path: string) { return invoke<AssetPayload>("read_asset", { path }); }

async function addImages() {
  const paths = await invoke<string[]>("pick_images");
  if (!paths.length) return;
  const existing = new Set(state.items.map((item) => item.path.toLowerCase()));
  const unique = paths.filter((path) => !existing.has(path.toLowerCase()));
  if (!unique.length) return;
  const batchId = `batch_${Date.now()}_${Math.random().toString(36).slice(2)}`;
  const items = await Promise.all(unique.map(async (path): Promise<QueueItem> => {
    let assetUrl = "";
    try { assetUrl = (await readAsset(path)).dataUrl; } catch { /* The runtime validates the source again. */ }
    const name = pathLeaf(path);
    return {
      id: `image_${Date.now()}_${Math.random().toString(36).slice(2)}`, batchId, path, name,
      extension: name.split(".").pop()?.toUpperCase() || t("image"), assetUrl,
      polycount: 5000, autoSegment: false, submitted: false, state: "queued",
    };
  }));
  state.items.push(...items);
  state.selectedId = items[0].id;
  renderQueue(); updateUi();
  await displayItem(items[0]);
}

function removeItem(id: string) {
  const index = state.items.findIndex((item) => item.id === id);
  if (index < 0 || state.items[index].state === "running") return;
  state.items.splice(index, 1);
  if (state.selectedId === id) state.selectedId = state.items[Math.min(index, state.items.length - 1)]?.id || "";
  renderQueue(); updateUi();
  const item = selectedItem();
  if (item) void displayItem(item);
}

async function selectItem(id: string) {
  state.selectedId = id;
  renderQueue(); updateUi();
  const item = selectedItem();
  if (item) await displayItem(item);
}

async function displayItem(item: QueueItem) {
  const token = ++state.displayToken;
  try {
    if (item.state === "done" && item.result?.outputPath) {
      const asset = await readAsset(item.result.outputPath);
      if (token !== state.displayToken || state.selectedId !== item.id) return;
      item.modelStats = await viewer.setModel(asset.dataUrl, Boolean(item.result.isSegmented));
      if (token !== state.displayToken || state.selectedId !== item.id) return;
      updateUi();
      return;
    }
    if (!item.assetUrl) item.assetUrl = (await readAsset(item.path)).dataUrl;
    if (token !== state.displayToken || state.selectedId !== item.id) return;
    await viewer.setSource(item.assetUrl);
  } catch { /* The status surface remains usable even if preview loading fails. */ }
  syncViewerControls();
}

async function loadDepthFor(item: QueueItem, path: string) {
  if (!path || item.loadedDepthPath === path || state.selectedId !== item.id) return;
  item.loadedDepthPath = path;
  try { await viewer.setDepth((await readAsset(path)).dataUrl); } catch { item.loadedDepthPath = ""; }
}

function friendlyError(message: string) {
  const text = message.toLowerCase();
  if (text.includes("rate limit") || text.includes("retry after")) return t("serviceBusy");
  if (text.includes("runtime_missing") || text.includes("runtime") && text.includes("missing")) return t("engineMissing");
  if (text.includes("timed out") || text.includes("timeout")) return t("timedOut");
  if (text.includes("segment")) return t("separationFailed");
  return t("interrupted");
}

function friendlyStatus() {
  const item = selectedItem();
  if (!item) return { title: t("ready"), detail: t("chooseToBegin"), stage: "idle" as Stage };
  if (item.state === "done") return {
    title: item.result?.isSegmented ? t("partsReady") : t("modelReady"),
    detail: item.result?.isSegmented ? t("dragInspectParts") : t("dragInspect"), stage: "done" as Stage,
  };
  if (item.state === "failed") return { title: t("couldNotCreate"), detail: friendlyError(item.result?.error || item.result?.progressText || ""), stage: "failed" as Stage };
  if (item.state === "cancelled") return { title: t("cancelled"), detail: t("cancelledDetail"), stage: "cancelled" as Stage };
  if (item.state === "queued" && item.submitted && activeJobCount()) return { title: t("queuedTitle"), detail: t("queuedDetail"), stage: "idle" as Stage };
  if (item.state !== "running") return { title: t("ready"), detail: t("adjustThenGenerate"), stage: "idle" as Stage };
  const status = item.result || state.backendStatus;
  if (status.workspaceState === "waiting") return { title: t("preparingWorkspace"), detail: t("finishingPreparation"), stage: status.stage };
  if (status.stage === "preparing") return { title: t("preparingWorkspace"), detail: t("gettingEverythingReady"), stage: status.stage };
  if (status.stage === "segmenting") return { title: t("separatingParts"), detail: t("findingPieces"), stage: status.stage };
  if (status.stage === "finalizing") return { title: t("finishingModel"), detail: t("preparingGeometry"), stage: status.stage };
  const details: Record<string, MessageKey> = {
    depth_preview: "readingDepth", model_setup: "preparingImage", model_creation: "shapingGeometry", separation: "findingPieces", finalizing: "preparingGeometry",
  };
  return { title: t("creatingModel"), detail: t(details[status.phase || ""] || (status.previewPath ? "shapingGeometry" : "readingDepth")), stage: status.stage };
}

function beginProgress(item: QueueItem, estimateMs: number) {
  item.operationStartedAt = Date.now(); item.estimatedTotalMs = estimateMs; item.displayedProgress = 0;
}

function formatRemaining(milliseconds: number) {
  if (milliseconds <= 15_000) return t("almostThere");
  if (milliseconds < 60_000) return t("lessMinute");
  return t("aboutMinutes", { count: Math.max(1, Math.ceil(milliseconds / 60_000)) });
}

function formatModelStats(stats: ModelStats) {
  const number = new Intl.NumberFormat(locale());
  return t("modelStats", { vertices: number.format(stats.vertices), faces: number.format(stats.faces) });
}

function updateProgressUi() {
  const item = selectedItem();
  const busy = item?.state === "running";
  nodes.progressTrack.classList.toggle("visible", busy); nodes.statusEta.classList.toggle("visible", busy);
  if (!busy) {
    const done = selectedItem()?.state === "done";
    nodes.progressTrack.setAttribute("aria-valuenow", done ? "100" : "0"); nodes.progressFill.style.width = done ? "100%" : "0%"; nodes.statusEta.textContent = ""; return;
  }
  if (!item) return;
  const elapsedMs = Math.max(0, Date.now() - (item.operationStartedAt || Date.now()));
  const estimateMs = Math.max(10_000, item.estimatedTotalMs || 240_000);
  const curved = Math.min(0.94, 0.9 * (1 - Math.exp((-3 * elapsedMs) / estimateMs)));
  const reported = Math.max(0, Math.min(0.94, item.result?.progressRatio || 0));
  item.displayedProgress = Math.max(item.displayedProgress || 0, curved, reported);
  const percent = Math.round(item.displayedProgress * 100);
  nodes.progressTrack.setAttribute("aria-valuenow", String(percent)); nodes.progressFill.style.width = `${percent}%`;
  nodes.statusEta.textContent = elapsedMs >= estimateMs ? t("takingLonger") : formatRemaining(estimateMs - elapsedMs);
}

function syncViewerControls() {
  const done = selectedItem()?.state === "done";
  nodes.viewerToolbar.classList.toggle("visible", Boolean(done));
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

function updateUi() {
  const item = selectedItem();
  const status = friendlyStatus();
  const busy = activeJobCount() > 0;
  const missing = item?.result?.runtimeStatus === "missing" || state.backendStatus.runtimeStatus === "missing";
  nodes.statusTitle.textContent = status.title; nodes.statusDetail.textContent = status.detail;
  nodes.stageStatus.dataset.stage = status.stage; nodes.statusMark.innerHTML = item?.state === "done" ? ICONS.check : ICONS.sparkle;
  nodes.readinessText.textContent = missing ? t("unavailable") : busy ? t("working") : state.preparationStatus === "ready" ? t("ready") : t("preparing");
  nodes.readiness.classList.toggle("busy", busy || state.preparationStatus === "preparing");
  nodes.readiness.classList.toggle("error", missing);
  nodes.sourceName.textContent = item ? stripExtension(item.name) : t("chooseImages");
  const selectedBatchSize = item ? batchItems(item.batchId).length : 0;
  nodes.sourceMeta.textContent = item
    ? selectedBatchSize > 1 ? t("sharedSettings", { count: selectedBatchSize }) : item.extension
    : t("formats");
  nodes.sourceThumb.innerHTML = item?.assetUrl ? `<img alt="" src="${item.assetUrl}">` : ICONS.image;
  nodes.folderName.textContent = state.outputDir || t("defaultFolder");
  nodes.folderName.title = state.outputDir;
  const polycount = item?.polycount ?? 5000;
  nodes.polycountValue.value = new Intl.NumberFormat(locale()).format(polycount); nodes.polycountRange.value = String(polycount);
  nodes.autoSegmentInput.checked = Boolean(item?.autoSegment);
  const locked = !item || item.state !== "queued" || item.submitted;
  nodes.polycountRange.disabled = locked; nodes.autoSegmentInput.disabled = locked;
  const selectedDraft = Boolean(item?.state === "queued" && !item.submitted);
  nodes.generateButton.disabled = !item || missing || item.state === "running";
  nodes.generateButton.classList.toggle("is-busy", busy);
  const rerun = item ? item.state === "done" || item.state === "failed" || item.state === "cancelled" : false;
  const selectedBatchSizeForAction = item ? batchItems(item.batchId).length : 0;
  nodes.generateLabel.textContent = selectedDraft && busy
    ? t("addToQueue")
    : item?.state === "running"
      ? t("creatingModel")
      : selectedDraft && selectedBatchSizeForAction > 1
        ? t("generateQueue")
        : rerun
          ? t("generateAgain")
          : t("generateModel");
  nodes.cancelButton.classList.toggle("visible", busy || state.queueActive);
  const canSegment = item?.state === "done" && item.result?.canSegment && item.result?.jobId && !item.result?.isSegmented && activeJobCount() < MAX_PARALLEL_JOBS;
  nodes.segmentButton.classList.toggle("visible", Boolean(canSegment));
  nodes.resultSummary.classList.toggle("visible", item?.state === "done");
  nodes.resultName.textContent = item?.result?.isSegmented ? t("partsReady") : t("modelReady");
  nodes.resultMeta.textContent = item?.result?.outputName || t("savedAutomatically");
  const showModelStats = item?.state === "done" && Boolean(item.modelStats);
  nodes.modelStats.textContent = item?.modelStats ? formatModelStats(item.modelStats) : "";
  nodes.modelStats.classList.toggle("visible", showModelStats);
  nodes.showFolderButton.classList.toggle("visible", Boolean(item?.state === "done" && item.result?.outputPath));
  nodes.emptyCopy.classList.toggle("hidden", Boolean(item));
  renderQueue(); syncViewerControls(); updateProgressUi();
}

function applyBackendStatus(item: QueueItem, status: JobStatus) {
  if (BUSY_STAGES.has(status.stage)) {
    if (!item.operationStartedAt) {
      item.operationStartedAt = Date.now() - Math.max(0, status.elapsedMs || 0);
      item.displayedProgress = Math.max(0, status.progressRatio || 0);
    }
    if (status.estimatedTotalMs) item.estimatedTotalMs = status.estimatedTotalMs;
  }
  if (state.selectedId === item.id) state.backendStatus = status;
  item.result = status;
  if (status.previewPath) void loadDepthFor(item, status.previewPath);
  updateUi();
}

async function waitForJob(item: QueueItem, initial: JobStatus) {
  let status = initial; applyBackendStatus(item, status);
  const jobId = status.jobId;
  if (!jobId) throw new Error("The model job did not return an ID.");
  while (BUSY_STAGES.has(status.stage)) {
    await delay(800);
    status = await invoke<JobStatus>("job_status", { jobId });
    applyBackendStatus(item, status);
  }
  return status;
}

async function runItem(item: QueueItem) {
  state.runningIds.add(item.id); item.state = "running";
  beginProgress(item, item.autoSegment ? 360_000 : 240_000);
  if (state.selectedId === item.id && item.assetUrl) await viewer.setSource(item.assetUrl);
  const request: StartJobRequest = {
    imagePath: item.path, outputDir: state.outputDir || null, polycount: Math.min(20000, Math.max(500, Math.round(item.polycount))),
    mode: "topology_mesh", outputFormat: "glb_plain", autoSegment: item.autoSegment, segmentationMode: item.autoSegment ? "parts" : "none",
  };
  try {
    const final = await waitForJob(item, await invoke<JobStatus>("start_job", request));
    item.result = final;
    if (final.stage === "done") {
      item.state = "done";
      if (state.selectedId === item.id) await displayItem(item);
    } else if (final.stage === "cancelled") item.state = "cancelled";
    else item.state = "failed";
  } catch (error) {
    item.state = "failed";
    item.result = { stage: "failed", progressText: String(error), error: String(error), runtimeStatus: state.backendStatus.runtimeStatus };
  } finally {
    state.runningIds.delete(item.id); updateUi(); startPreparationPolling();
  }
}

async function processQueue() {
  if (state.queueActive) return;
  const selected = selectedItem();
  if (!pendingItems().length && selected && selected.state !== "running") {
    selected.state = "queued"; selected.result = undefined;
  }
  state.queueActive = true; state.cancelRequested = false; updateUi();
  const active = new Map<string, Promise<void>>();
  while (!state.cancelRequested) {
    while (activeJobCount() < MAX_PARALLEL_JOBS) {
      const next = pendingItems()[0];
      if (!next) break;
      const operation = runItem(next).finally(() => active.delete(next.id));
      active.set(next.id, operation);
    }
    if (!active.size) {
      if (pendingItems().length && activeJobCount() >= MAX_PARALLEL_JOBS) {
        await delay(400);
        continue;
      }
      break;
    }
    await Promise.race(active.values());
  }
  await Promise.allSettled(active.values());
  state.queueActive = false; state.cancelRequested = false; updateUi();
}

function submitSelectedBatch() {
  const item = selectedItem();
  if (!item) return;
  if (item.state === "queued" && !item.submitted) {
    for (const member of batchItems(item.batchId)) {
      if (member.state === "queued") member.submitted = true;
    }
  } else if (item.state === "done" || item.state === "failed" || item.state === "cancelled") {
    item.state = "queued";
    item.submitted = true;
    item.result = undefined;
    item.modelStats = undefined;
  }
  updateUi();
  if (!state.queueActive) void processQueue();
}

async function segmentSelected() {
  const item = selectedItem();
  if (!item?.result?.jobId || !item.result.canSegment || activeJobCount() >= MAX_PARALLEL_JOBS) return;
  const continuationId = item.result.jobId;
  state.runningIds.add(item.id); item.state = "running"; beginProgress(item, 120_000); updateUi();
  try {
    const final = await waitForJob(item, await invoke<JobStatus>("segment_model", { continuationId }));
    item.result = final; item.state = final.stage === "done" ? "done" : "failed";
    if (item.state === "done") await displayItem(item);
  } catch (error) {
    item.state = "failed"; item.result = { stage: "failed", progressText: String(error), error: String(error) };
  } finally {
    state.runningIds.delete(item.id); updateUi(); startPreparationPolling();
    if (pendingItems().length && !state.queueActive) void processQueue();
  }
}

function startPreparationPolling() {
  window.clearTimeout(state.preparationTimer);
  const token = ++state.preparationPollToken;
  const check = async () => {
    try { state.preparationStatus = await invoke<string>("runtime_preparation_status"); } catch { state.preparationStatus = "not_ready"; }
    if (token !== state.preparationPollToken) return;
    updateUi();
    const delayMs = state.preparationStatus === "preparing" || state.preparationStatus === "not_ready" ? 1000 : 15_000;
    state.preparationTimer = window.setTimeout(check, delayMs);
  };
  void check();
}

async function restoreCurrentJobs() {
  try {
    const statuses = await invoke<JobStatus[]>("job_statuses");
    const recoverable = new Map<string, JobStatus>();
    for (const status of statuses) {
      if (status.sourceImagePath && (BUSY_STAGES.has(status.stage) || status.stage === "done")) {
        recoverable.set(status.sourceImagePath, status);
      }
    }
    const items = await Promise.all([...recoverable.values()].map(async (status, index): Promise<QueueItem> => {
      const path = status.sourceImagePath!;
      let assetUrl = ""; try { assetUrl = (await readAsset(path)).dataUrl; } catch { /* Status remains recoverable. */ }
      const name = pathLeaf(path);
      const running = BUSY_STAGES.has(status.stage);
      return {
        id: `recovered_${Date.now()}_${index}`, batchId: `recovered_batch_${Date.now()}_${index}`, path, name,
        extension: name.split(".").pop()?.toUpperCase() || t("image"), assetUrl, polycount: 5000,
        autoSegment: Boolean(status.isSegmented), submitted: true, state: running ? "running" : "done", result: status,
        operationStartedAt: running ? Date.now() - Math.max(0, status.elapsedMs || 0) : undefined,
        estimatedTotalMs: status.estimatedTotalMs || 240_000, displayedProgress: status.progressRatio || 0,
      };
    }));
    if (!items.length) { updateUi(); return; }
    const latest = items[items.length - 1];
    state.items.push(...items); state.selectedId = latest.id;
    state.backendStatus = latest.result!;
    for (const item of items) if (item.state === "running") state.runningIds.add(item.id);
    updateUi(); await displayItem(latest);
    await Promise.all(items.filter((item) => item.state === "running").map(async (item) => {
      try {
        const final = await waitForJob(item, item.result!);
        item.result = final;
        item.state = final.stage === "done" ? "done" : final.stage === "cancelled" ? "cancelled" : "failed";
        if (state.selectedId === item.id && item.state === "done") await displayItem(item);
      } catch (error) {
        item.state = "failed";
        item.result = { stage: "failed", progressText: String(error), error: String(error) };
      } finally {
        state.runningIds.delete(item.id);
        updateUi();
      }
    }));
  } catch { updateUi(); }
}

async function loadDevModelPreview(modelUrl: string) {
  try {
    const response = await fetch(modelUrl);
    if (!response.ok) throw new Error(`Preview model returned ${response.status}`);
    const objectUrl = URL.createObjectURL(await response.blob());
    const name = pathLeaf(modelUrl);
    const item: QueueItem = {
      id: "dev_model", batchId: "dev_batch", path: modelUrl, name, extension: "GLB", assetUrl: "", polycount: 5000,
      autoSegment: devParams?.get("segmented") === "1", submitted: true, state: "done",
      result: { stage: "done", progressText: "", outputPath: modelUrl, outputName: name, isSegmented: devParams?.get("segmented") === "1", canSegment: false },
    };
    state.items.push(item); state.selectedId = item.id;
    item.modelStats = await viewer.setModel(objectUrl, Boolean(item.result?.isSegmented));
    updateUi();
  } catch (error) {
    state.backendStatus = { stage: "failed", progressText: String(error), error: String(error) };
    updateUi();
  }
}

function loadDevBatchPreview() {
  const makeItem = (
    id: string,
    batchId: string,
    name: string,
    itemState: QueueState,
    submitted: boolean,
  ): QueueItem => ({
    id, batchId, path: name, name, extension: "PNG", assetUrl: "", polycount: batchId === "batch_2" ? 8200 : 5000,
    autoSegment: batchId === "batch_2", submitted, state: itemState,
  });
  state.items.push(
    makeItem("batch_1_a", "batch_1", "atrium-front.png", "running", true),
    makeItem("batch_1_b", "batch_1", "atrium-side.png", "running", true),
    makeItem("batch_2_a", "batch_2", "character-front.png", "queued", false),
    makeItem("batch_2_b", "batch_2", "character-side.png", "queued", false),
    makeItem("batch_2_c", "batch_2", "character-back.png", "queued", false),
  );
  state.selectedId = "batch_2_a";
  state.runningIds.add("batch_1_a");
  state.runningIds.add("batch_1_b");
  state.queueActive = true;
  state.items[0].operationStartedAt = Date.now() - 42_000;
  state.items[0].estimatedTotalMs = 120_000;
  state.items[0].displayedProgress = 0.38;
  state.backendStatus = {
    jobId: "dev_running",
    stage: "generating",
    phase: "model_creation",
    progressText: "",
    runtimeStatus: "installed",
    progressRatio: 0.38,
    estimatedTotalMs: 120_000,
  };
  updateUi();
}

function loadDevParallelHarness() {
  const harness = { starts: [] as string[], active: 0, maxActive: 0, completed: 0 };
  const polls = new Map<string, number>();
  const syncHarness = () => {
    document.documentElement.dataset.parallelStarts = String(harness.starts.length);
    document.documentElement.dataset.parallelActive = String(harness.active);
    document.documentElement.dataset.parallelMax = String(harness.maxActive);
    document.documentElement.dataset.parallelCompleted = String(harness.completed);
  };
  window.__SGT_PARALLEL_TEST__ = harness;
  syncHarness();
  window.invoke = async <T>(cmd: string, args?: unknown): Promise<T> => {
    if (cmd === "start_job") {
      const jobId = `parallel_${harness.starts.length + 1}`;
      harness.starts.push(jobId); harness.active += 1; harness.maxActive = Math.max(harness.maxActive, harness.active);
      syncHarness();
      polls.set(jobId, 0);
      return { jobId, stage: "generating", progressText: "", runtimeStatus: "installed" } as T;
    }
    if (cmd === "job_status") {
      const jobId = (args as { jobId?: string })?.jobId || "";
      const count = (polls.get(jobId) || 0) + 1; polls.set(jobId, count);
      if (count < 2) return { jobId, stage: "generating", progressText: "", runtimeStatus: "installed", progressRatio: 0.5 } as T;
      harness.active -= 1; harness.completed += 1;
      syncHarness();
      return { jobId, stage: "done", progressText: "", runtimeStatus: "installed", isSegmented: false } as T;
    }
    if (cmd === "read_asset") throw new Error("No fixture asset");
    return null as T;
  };
  const batchId = "parallel_batch";
  state.items.push(
    { id: "parallel_a", batchId, path: "parallel-a.png", name: "parallel-a.png", extension: "PNG", assetUrl: "", polycount: 5000, autoSegment: false, submitted: true, state: "queued" },
    { id: "parallel_b", batchId, path: "parallel-b.png", name: "parallel-b.png", extension: "PNG", assetUrl: "", polycount: 5000, autoSegment: true, submitted: true, state: "queued" },
  );
  state.selectedId = "parallel_a";
  updateUi();
  void processQueue();
}

window.applyHostContext = (context) => {
  const theme = context.theme === "light" ? "light" : "dark";
  document.documentElement.dataset.theme = theme; setLocale(context.language); viewer.setTheme(theme);
  applyTranslations(); updateUi();
};

nodes.dragRegion.addEventListener("pointerdown", (event) => {
  if ((event.target as HTMLElement).closest("button, input, label")) return;
  void invoke("start_drag");
});
nodes.minimizeButton.addEventListener("click", () => void invoke("minimize_window"));
nodes.closeButton.addEventListener("click", () => void invoke("close_window"));
nodes.addImagesButton.addEventListener("click", () => void addImages());
nodes.chooseImageButton.addEventListener("click", () => void addImages());
nodes.chooseFolderButton.addEventListener("click", async () => { const path = await invoke<string | null>("pick_output_dir"); if (path) { state.outputDir = path; updateUi(); } });
nodes.showFolderButton.addEventListener("click", () => void invoke("open_output", { kind: "folder", path: selectedItem()?.result?.outputPath || null }));
nodes.generateButton.addEventListener("click", submitSelectedBatch);
nodes.segmentButton.addEventListener("click", () => void segmentSelected());
nodes.cancelButton.addEventListener("click", async () => { state.cancelRequested = true; await invoke("cancel_job"); });
nodes.polycountRange.addEventListener("input", () => {
  const item = selectedItem();
  if (item?.state === "queued" && !item.submitted) {
    const value = Number(nodes.polycountRange.value);
    batchItems(item.batchId).forEach((member) => { if (member.state === "queued" && !member.submitted) member.polycount = value; });
    updateUi();
  }
});
nodes.autoSegmentInput.addEventListener("change", () => {
  const item = selectedItem();
  if (item?.state === "queued" && !item.submitted) {
    batchItems(item.batchId).forEach((member) => { if (member.state === "queued" && !member.submitted) member.autoSegment = nodes.autoSegmentInput.checked; });
    updateUi();
  }
});
nodes.shadingButtons.forEach((button) => button.addEventListener("click", () => { viewer.setShading(button.dataset.shading as ShadingMode); syncViewerControls(); }));
nodes.outlineButton.addEventListener("click", () => { state.outline = !state.outline; viewer.setOutline(state.outline); syncViewerControls(); });
nodes.rotateButton.addEventListener("click", () => { state.rotate = !state.rotate; viewer.setAutoRotate(state.rotate); syncViewerControls(); });
nodes.gridButton.addEventListener("click", () => { state.grid = !state.grid; viewer.setGrid(state.grid); syncViewerControls(); });
nodes.wireButton.addEventListener("click", () => { state.wire = !state.wire; viewer.setWireframe(state.wire); syncViewerControls(); });
nodes.fitButton.addEventListener("click", () => viewer.fitView());

applyTranslations(); updateUi(); window.setInterval(updateProgressUi, 250);
async function loadDefaultOutputDir() {
  try {
    state.outputDir = await invoke<string>("default_output_dir");
    updateUi();
  } catch {
    // Browser-only previews have no native output directory.
  }
}

const devModelUrl = devParams?.get("model");
if (devParams?.get("output")) {
  state.outputDir = devParams.get("output") || "";
  updateUi();
}
if (devParams?.get("parallel") === "1") {
  loadDevParallelHarness();
} else if (devParams?.get("batches") === "1") {
  loadDevBatchPreview();
} else if (devModelUrl) {
  void loadDevModelPreview(devModelUrl);
} else if (window.invoke) {
  void loadDefaultOutputDir();
  void invoke("prepare_runtime").catch(() => undefined).finally(startPreparationPolling);
  void restoreCurrentJobs();
}
