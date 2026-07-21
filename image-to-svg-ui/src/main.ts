import "./styles.css";
import { setLanguage, t, type MessageKey } from "./i18n";

type Model = "simple" | "detail";
type Stage = "draft" | "queued" | "preparing" | "generating" | "finalizing" | "done" | "failed" | "cancelled";
type HostContext = { theme?: "light" | "dark"; language?: string };
type Asset = { dataUrl?: string; text?: string; sizeBytes: number };
type HistoryEntry = {
  id: string; tool: "svg"; sourcePath: string; outputPath: string; outputName: string;
  createdAtMs: number; metadata?: { model?: Model };
};
type JobStatus = {
  jobId: string; stage: Stage; progressText: string; elapsedMs?: number; estimatedTotalMs?: number;
  progressRatio?: number; outputPath?: string; outputName?: string; sourceImagePath: string;
  model: Model; creditsRemaining?: number; error?: string; progressKey?: string; phase?: string; previewPath?: string;
};
type Item = {
  id: string; batchId: string; path: string; name: string; model: Model; outputDir: string;
  stage: Stage; jobId?: string; progress?: number; progressText?: string; outputPath?: string;
  outputName?: string; error?: string; sourceUrl?: string; svgText?: string; pathCount?: number;
  progressKey?: string; phase?: string; previewPath?: string; depthUrl?: string;
  operationStartedAt?: number; estimatedTotalMs?: number; displayedProgress?: number;
  dirty?: boolean; saveError?: boolean; undoStack?: string[]; redoStack?: string[]; savedSvgText?: string;
  originalWidth?: string; originalHeight?: string;
  historyId?: string; createdAtMs?: number;
};

declare global {
  interface Window {
    invoke?: <T = unknown>(cmd: string, args?: unknown) => Promise<T>;
    __SGT_CONTEXT__?: HostContext;
    applyHostContext?: (context: HostContext) => void;
    handleNativeFileDrop?: (paths: string[]) => void;
    handleNativeFileDrag?: (active: boolean) => void;
  }
}

const context = window.__SGT_CONTEXT__ || {};
const pageParams = new URLSearchParams(location.search);
const activeLanguage = pageParams.get("lang") || context.language || "en";
setLanguage(activeLanguage);
document.documentElement.dataset.theme = (import.meta.env.DEV && pageParams.get("theme")) || context.theme || "dark";

function invoke<T>(cmd: string, args: unknown = {}): Promise<T> {
  if (window.invoke) return window.invoke<T>(cmd, args);
  return Promise.reject(new Error("Desktop bridge unavailable"));
}

function icon(path: string) {
  return `<svg aria-hidden="true" viewBox="0 -960 960 960"><path d="${path}"/></svg>`;
}
const I = {
  vector: icon("M240-120q-50 0-85-35t-35-85q0-50 35-85t85-35h80v-240h-80q-50 0-85-35t-35-85q0-50 35-85t85-35q50 0 85 35t35 85v80h240v-80q0-50 35-85t85-35q50 0 85 35t35 85q0 50-35 85t-85 35h-80v240h80q50 0 85 35t35 85q0 50-35 85t-85 35q-50 0-85-35t-35-85v-80H360v80q0 50-35 85t-85 35Zm0-80q17 0 28.5-11.5T280-240q0-17-11.5-28.5T240-280q-17 0-28.5 11.5T200-240q0 17 11.5 28.5T240-200Zm0-480q17 0 28.5-11.5T280-720q0-17-11.5-28.5T240-760q-17 0-28.5 11.5T200-720q0 17 11.5 28.5T240-680Zm480 480q17 0 28.5-11.5T760-240q0-17-11.5-28.5T720-280q-17 0-28.5 11.5T680-240q0 17 11.5 28.5T720-200Zm0-480q17 0 28.5-11.5T760-720q0-17-11.5-28.5T720-760q-17 0-28.5 11.5T680-720q0 17 11.5 28.5T720-680ZM360-360h240v-240H360v240Z"),
  image: icon("M200-120q-33 0-56.5-23.5T120-200v-560q0-33 23.5-56.5T200-840h560q33 0 56.5 23.5T840-760v560q0 33-23.5 56.5T760-120H200Zm80-160h400q12 0 18-11t-2-21L586-459q-6-8-16-8t-16 8L450-320l-74-99q-6-8-16-8t-16 8l-80 107q-8 10-2 21t18 11Z"),
  add: icon("M440-440H240q-17 0-28.5-11.5T200-480q0-17 11.5-28.5T240-520h200v-200q0-17 11.5-28.5T480-760q17 0 28.5 11.5T520-720v200h200q17 0 28.5 11.5T760-480q0 17-11.5 28.5T720-440H520v200q0 17-11.5 28.5T480-200q-17 0-28.5-11.5T440-240v-200Z"),
  folder: icon("M160-160q-33 0-56.5-23.5T80-240v-480q0-33 23.5-56.5T160-800h207q16 0 30.5 6t25.5 17l57 57h320q33 0 56.5 23.5T880-640v400q0 33-23.5 56.5T800-160H160Z"),
  sparkle: icon("M260-380 100-453q-17-8-17-27t17-27l160-73 73-160q8-17 27-17t27 17l73 160 160 73q17 8 17 27t-17 27l-160 73-73 160q-8 17-27 17t-27-17l-73-160Z"),
  close: icon("M480-424 284-228q-11 11-28 11t-28-11q-11-11-11-28t11-28l196-196-196-196q-11-11-11-28t11-28q11-11 28-11t28 11l196 196 196-196q11-11 28-11t28 11q11 11 11 28t-11 28L536-480l196 196q11 11 11 28t-11 28q-11 11-28 11t-28-11L480-424Z"),
  minimize: icon("M240-440q-17 0-28.5-11.5T200-480q0-17 11.5-28.5T240-520h480q17 0 28.5 11.5T760-480q0 17-11.5 28.5T720-440H240Z"),
  zoomIn: icon("M784-120 532-372q-30 24-68.5 38T380-320q-109 0-184.5-75.5T120-580q0-109 75.5-184.5T380-840q109 0 184.5 75.5T640-580q0 45-14 83.5T588-428l252 252-56 56ZM380-400q75 0 127.5-52.5T560-580q0-75-52.5-127.5T380-760q-75 0-127.5 52.5T200-580q0 75 52.5 127.5T380-400Zm-40-60v-80h-80v-80h80v-80h80v80h80v80h-80v80h-80Z"),
  zoomOut: icon("M784-120 532-372q-30 24-68.5 38T380-320q-109 0-184.5-75.5T120-580q0-109 75.5-184.5T380-840q109 0 184.5 75.5T640-580q0 45-14 83.5T588-428l252 252-56 56ZM380-400q75 0 127.5-52.5T560-580q0-75-52.5-127.5T380-760q-75 0-127.5 52.5T200-580q0 75 52.5 127.5T380-400ZM260-540v-80h240v80H260Z"),
  fit: icon("M200-120q-33 0-56.5-23.5T120-200v-160q0-17 11.5-28.5T160-400q17 0 28.5 11.5T200-360v160h160q17 0 28.5 11.5T400-160q0 17-11.5 28.5T360-120H200Zm560 0H600q-17 0-28.5-11.5T560-160q0-17 11.5-28.5T600-200h160v-160q0-17 11.5-28.5T800-400q17 0 28.5 11.5T840-360v160q0 33-23.5 56.5T760-120ZM160-560q-17 0-28.5-11.5T120-600v-160q0-33 23.5-56.5T200-840h160q17 0 28.5 11.5T400-800q0 17-11.5 28.5T360-760H200v160q0 17-11.5 28.5T160-560Zm640 0q-17 0-28.5-11.5T760-600v-160H600q-17 0-28.5-11.5T560-800q0-17 11.5-28.5T600-840h160q33 0 56.5 23.5T840-760v160q0 17-11.5 28.5T800-560Z"),
  checker: icon("M120-120v-720h720v720H120Zm80-480h160v-160H200v160Zm240 0h160v-160H440v160Zm240 0h80v-160h-80v160ZM200-360h160v-160H200v160Zm240 0h160v-160H440v160Zm240 0h80v-160h-80v160ZM200-200h160v-80H200v80Zm240 0h160v-80H440v80Zm240 0h80v-80h-80v80Z"),
  outline: icon("M200-120q-33 0-56.5-23.5T120-200v-560q0-33 23.5-56.5T200-840h560q33 0 56.5 23.5T840-760v560q0 33-23.5 56.5T760-120H200Zm0-80h560v-560H200v560Zm80-80v-400h400v400H280Z"),
  undo: icon("M280-200v-80h284q63 0 109.5-40T720-420q0-60-46.5-100T564-560H312l104 104-56 56-200-200 200-200 56 56-104 104h252q97 0 166.5 63T800-420q0 94-69.5 157T564-200H280Z"),
  redo: icon("M680-200H396q-97 0-166.5-63T160-420q0-94 69.5-157T396-640h252L544-744l56-56 200 200-200 200-56-56 104-104H396q-63 0-109.5 40T240-420q0 60 46.5 100T396-280h284v80Z"),
  trash: icon("M280-120q-33 0-56.5-23.5T200-200v-520h-40v-80h200v-40h240v40h200v80h-40v520q0 33-23.5 56.5T680-120H280Zm80-160h80v-360h-80v360Zm160 0h80v-360h-80v360Z"),
  rename: icon("M200-120q-33 0-56.5-23.5T120-200v-113q0-16 6-30.5t17-25.5l407-407q23-23 57.5-23t57.5 23l56 57q23 23 23 57t-23 57L369-143q-11 11-25.5 17t-30.5 6H200Zm400-600L200-320v120h120l400-400-120-120Z"),
  save: icon("M200-120q-33 0-56.5-23.5T120-200v-560q0-33 23.5-56.5T200-840h447q16 0 30.5 6t25.5 17l114 114q11 11 17 25.5t6 30.5v447q0 33-23.5 56.5T760-120H200Zm280-120q50 0 85-35t35-85q0-50-35-85t-85-35q-50 0-85 35t-35 85q0 50 35 85t85 35ZM240-600h360v-160H240v160Z"),
};

const app = document.querySelector<HTMLElement>("#app")!;
app.innerHTML = `
<section class="shell">
  <div class="drop-overlay" id="dropOverlay">${I.image}<strong>${t("dropImages")}</strong></div>
  <header class="titlebar" id="dragRegion">
    <div class="identity"><span class="app-icon">${I.vector}</span><strong>${t("title")}</strong><span class="readiness" id="readiness"><i></i><span id="readyText">${t("preparing")}</span></span></div>
    <div class="window-actions"><button class="icon-button" id="minimize" title="${t("minimize")}">${I.minimize}</button><button class="icon-button close" id="close" title="${t("close")}">${I.close}</button></div>
  </header>
  <main class="workspace">
    <aside class="queue-rail">
      <div class="rail-heading"><span>${t("queue")}</span><button class="icon-button add" id="addImages" title="${t("addImages")}">${I.add}</button></div>
      <div class="queue-list" id="queueList"></div>
    </aside>
    <section class="stage">
      <div class="artboard-wrap">
        <div class="artboard" id="artboard"><div class="empty-state">${I.vector}<strong>${t("canvasEmpty")}</strong><span>${t("canvasHint")}</span></div></div>
        <div class="canvas-toolbar" id="viewerToolbar" hidden><button class="view-button" id="zoomOut" title="${t("zoomOut")}">${I.zoomOut}</button><output id="zoomValue">100%</output><button class="view-button" id="zoomIn" title="${t("zoomIn")}">${I.zoomIn}</button><button class="view-button" id="fitView" title="${t("fitView")}">${I.fit}</button><i></i><button class="view-button" id="canvasBackground" title="${t("canvasBackground")}">${I.checker}</button><button class="view-button" id="showOutlines" title="${t("showOutlines")}">${I.outline}</button></div>
        <section class="edit-toolbar" id="editSection" hidden><small id="selectionLabel">${t("noSelection")}</small><i></i><label class="paint-control"><span>${t("fill")}</span><input id="fillColor" type="color" value="#315fce" title="${t("fill")}" disabled></label><button class="paint-none" id="removeFill" title="${t("removeFill")}" disabled>${I.close}</button><label class="paint-control"><span>${t("stroke")}</span><input id="strokeColor" type="color" value="#252c39" title="${t("stroke")}" disabled></label><button class="paint-none" id="removeStroke" title="${t("removeStroke")}" disabled>${I.close}</button><i></i><div class="edit-actions"><button id="undoEdit" title="${t("undo")}" disabled>${I.undo}</button><button id="redoEdit" title="${t("redo")}" disabled>${I.redo}</button><button id="deleteShape" title="${t("deleteShape")}" disabled>${I.trash}</button><button class="save-edit" id="saveEdits" title="${t("saveChanges")}" disabled>${I.save}</button></div></section>
        <div class="status-strip" id="statusStrip"><span class="status-icon">${I.sparkle}</span><span class="status-copy"><span class="status-heading"><strong id="statusTitle">${t("selectJob")}</strong><small class="status-eta" id="statusEta"></small></span><small id="statusDetail"></small></span><i class="progress" id="progressTrack" role="progressbar" aria-valuemin="0" aria-valuemax="100"><b id="progressFill"></b></i></div>
      </div>
      <div class="result-meta" id="resultMeta"></div>
    </section>
    <aside class="controls">
      <section><span class="label">${t("source")}</span><button class="source-button" id="chooseImages"><span class="source-thumb" id="sourceThumb">${I.image}</span><span><strong id="sourceName">${t("addImages")}</strong><small id="sourceMeta"></small></span></button></section>
      <section><span class="label">${t("model")}</span><div class="model-control"><button data-model="simple" class="active"><strong>${t("simple")}</strong><small>${t("simpleHint")}</small></button><button data-model="detail"><strong>${t("detail")}</strong><small>${t("detailHint")}</small></button></div></section>
      <section><span class="label">${t("saveTo")}</span><button class="folder-row" id="chooseFolder">${I.folder}<span id="folderPath"></span></button></section>
      <div class="action-area"><button class="primary" id="generate">${I.sparkle}<span>${t("generate")}</span></button><button class="secondary" id="cancel">${t("cancel")}</button><button class="secondary" id="openFolder">${I.folder}<span>${t("openFolder")}</span></button></div>
    </aside>
  </main>
</section>`;

const q = <T extends Element>(selector: string) => document.querySelector<T>(selector)!;
const queueList = q<HTMLElement>("#queueList");
const artboard = q<HTMLElement>("#artboard");
const folderPath = q<HTMLElement>("#folderPath");
const sourceName = q<HTMLElement>("#sourceName");
const sourceMeta = q<HTMLElement>("#sourceMeta");
const sourceThumb = q<HTMLElement>("#sourceThumb");
const statusTitle = q<HTMLElement>("#statusTitle");
const statusDetail = q<HTMLElement>("#statusDetail");
const statusStrip = q<HTMLElement>("#statusStrip");
const statusEta = q<HTMLElement>("#statusEta");
const progressTrack = q<HTMLElement>("#progressTrack");
const progressFill = q<HTMLElement>("#progressFill");
const resultMeta = q<HTMLElement>("#resultMeta");
const viewerToolbar = q<HTMLElement>("#viewerToolbar");
const zoomValue = q<HTMLOutputElement>("#zoomValue");
const editSection = q<HTMLElement>("#editSection");
const selectionLabel = q<HTMLElement>("#selectionLabel");
const fillColor = q<HTMLInputElement>("#fillColor");
const strokeColor = q<HTMLInputElement>("#strokeColor");
const removeFill = q<HTMLButtonElement>("#removeFill");
const removeStroke = q<HTMLButtonElement>("#removeStroke");
const undoEdit = q<HTMLButtonElement>("#undoEdit");
const redoEdit = q<HTMLButtonElement>("#redoEdit");
const deleteShape = q<HTMLButtonElement>("#deleteShape");
const saveEdits = q<HTMLButtonElement>("#saveEdits");

let items: Item[] = [];
let selectedId = "";
let outputDir = "";
let defaultModel: Model = "simple";
let pumping = false;
let renderedOutput = "";
let displayVersion = 0;
let artboardResizeObserver: ResizeObserver | undefined;
let depthAnimationFrame = 0;
let depthPreviewVersion = 0;
let activeSvg: SVGSVGElement | undefined;
let activeViewport: HTMLElement | undefined;
let selectedShape: SVGGraphicsElement | undefined;
let viewScale = 1;
let viewX = 0;
let viewY = 0;
let viewBaseWidth = 0;
let viewBaseHeight = 0;
let outlineVisible = false;
let backgroundMode = 0;
let panStart: { x: number; y: number; viewX: number; viewY: number } | undefined;
let panMoved = false;
let renamingId = "";
let historyRefreshing = false;

function basename(path: string) { return path.split(/[\\/]/).pop() || path; }
function selected() { return items.find((item) => item.id === selectedId); }
function busy(item: Item) { return ["preparing", "generating", "finalizing"].includes(item.stage); }
function stageLabel(stage: Stage) {
  if (stage === "done") return t("done");
  if (stage === "failed") return t("failed");
  if (stage === "cancelled") return t("cancelled");
  if (stage === "draft") return t("selected");
  if (stage === "queued") return t("queued");
  return t("creating");
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

function fallbackEstimate(item: Item) { return item.model === "detail" ? 70_000 : 45_000; }

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
  statusEta.textContent = elapsedMs >= estimateMs ? t("takingLonger") : formatRemaining(estimateMs - elapsedMs);
}

async function loadSource(item: Item) {
  if (item.sourceUrl) return item.sourceUrl;
  const asset = await invoke<Asset>("read_asset", { path: item.path });
  item.sourceUrl = asset.dataUrl;
  return item.sourceUrl || "";
}

function sanitizeSvg(text: string): SVGSVGElement {
  const doc = new DOMParser().parseFromString(text, "image/svg+xml");
  if (doc.querySelector("parsererror") || doc.documentElement.tagName.toLowerCase() !== "svg") throw new Error("Invalid SVG result");
  doc.querySelectorAll("script, foreignObject, iframe, object, embed").forEach((node) => node.remove());
  doc.querySelectorAll("*").forEach((node) => {
    for (const attr of [...node.attributes]) {
      const name = attr.name.toLowerCase();
      const value = attr.value.trim();
      if (name.startsWith("on") || ((name === "href" || name === "xlink:href" || name === "src") && !value.startsWith("#") && !value.startsWith("data:"))) node.removeAttribute(attr.name);
    }
  });
  return document.importNode(doc.documentElement, true) as unknown as SVGSVGElement;
}

const EDITABLE_SELECTOR = "path,rect,circle,ellipse,polygon,polyline,line";
const pathAnimationState = new WeakMap<SVGPathElement, { stroke: string; fillOpacity: string; dashArray: string; dashOffset: string }>();

function restoreAnimatedPath(path: SVGPathElement) {
  const state = pathAnimationState.get(path);
  if (!state) return;
  path.getAnimations().forEach((animation) => animation.cancel());
  const restore = (property: string, value: string) => value ? path.style.setProperty(property, value) : path.style.removeProperty(property);
  restore("stroke", state.stroke);
  restore("fill-opacity", state.fillOpacity);
  restore("stroke-dasharray", state.dashArray);
  restore("stroke-dashoffset", state.dashOffset);
  pathAnimationState.delete(path);
}

function finishPathAnimations(svg = activeSvg) {
  svg?.querySelectorAll<SVGPathElement>("path").forEach(restoreAnimatedPath);
}

function animatePaths(svg: SVGSVGElement) {
  if (matchMedia("(prefers-reduced-motion: reduce)").matches) return;
  const paths = [...svg.querySelectorAll<SVGPathElement>("path")];
  const totalDuration = Math.min(6800, Math.max(2600, 1800 + Math.log2(paths.length + 1) * 430));
  const delayWindow = totalDuration * 0.34;
  const baseDuration = totalDuration - delayWindow;
  paths.forEach((path, index) => {
    let length = 0;
    try { length = path.getTotalLength(); } catch { return; }
    if (!Number.isFinite(length) || length <= 0) return;
    const computed = getComputedStyle(path);
    pathAnimationState.set(path, {
      stroke: path.style.stroke,
      fillOpacity: path.style.fillOpacity,
      dashArray: path.style.strokeDasharray,
      dashOffset: path.style.strokeDashoffset,
    });
    path.style.stroke = computed.stroke === "none" ? "var(--ink-accent)" : computed.stroke;
    path.style.strokeDasharray = `${length}`;
    path.style.strokeDashoffset = `${length}`;
    path.style.fillOpacity = "0";
    const animation = path.animate(
      [{ strokeDashoffset: length, fillOpacity: 0 }, { strokeDashoffset: 0, fillOpacity: 0, offset: 0.78 }, { strokeDashoffset: 0, fillOpacity: 1 }],
      {
        duration: baseDuration * (0.72 + Math.min(length / 700, 1) * 0.28),
        delay: paths.length > 1 ? (index / (paths.length - 1)) * delayWindow : 0,
        easing: "cubic-bezier(.2,.75,.25,1)", fill: "forwards",
      },
    );
    animation.finished.then(() => restoreAnimatedPath(path)).catch(() => undefined);
  });
}

function applyViewTransform() {
  if (!activeViewport) return;
  const width = viewBaseWidth * viewScale;
  const height = viewBaseHeight * viewScale;
  activeViewport.style.width = `${width}px`;
  activeViewport.style.height = `${height}px`;
  activeViewport.style.left = `${Math.round((artboard.clientWidth - width) / 2 + viewX)}px`;
  activeViewport.style.top = `${Math.round((artboard.clientHeight - height) / 2 + viewY)}px`;
  zoomValue.value = `${Math.round(viewScale * 100)}%`;
}

function resetView() {
  viewScale = 1; viewX = 0; viewY = 0;
  applyViewTransform();
}

function setZoom(next: number, anchor = { x: 0, y: 0 }) {
  const scale = Math.min(8, Math.max(0.25, next));
  const contentX = (anchor.x - viewX) / viewScale;
  const contentY = (anchor.y - viewY) / viewScale;
  viewX = anchor.x - contentX * scale;
  viewY = anchor.y - contentY * scale;
  viewScale = scale;
  applyViewTransform();
}

function syncCanvasModes() {
  artboard.classList.toggle("background-checker", backgroundMode === 0);
  artboard.classList.toggle("background-light", backgroundMode === 1);
  artboard.classList.toggle("background-dark", backgroundMode === 2);
  activeSvg?.classList.toggle("viewer-outlines", outlineVisible);
  q("#showOutlines").classList.toggle("active", outlineVisible);
  q("#canvasBackground").classList.toggle("active", backgroundMode !== 1);
}

function colorToHex(value: string, fallback: string) {
  if (/^#[0-9a-f]{6}$/i.test(value)) return value;
  if (/^#[0-9a-f]{3}$/i.test(value)) return `#${value.slice(1).split("").map((part) => part + part).join("")}`;
  const match = value.match(/rgba?\(\s*(\d+)[, ]+\s*(\d+)[, ]+\s*(\d+)/i);
  return match ? `#${match.slice(1, 4).map((part) => Number(part).toString(16).padStart(2, "0")).join("")}` : fallback;
}

function serializeActiveSvg(item = selected()) {
  if (!activeSvg || !item) return item?.svgText || "";
  finishPathAnimations(activeSvg);
  const clone = activeSvg.cloneNode(true) as SVGSVGElement;
  clone.classList.remove("viewer-outlines");
  clone.querySelectorAll(".vector-selected").forEach((element) => element.classList.remove("vector-selected"));
  if (!clone.getAttribute("class")) clone.removeAttribute("class");
  clone.querySelectorAll("[class='']").forEach((element) => element.removeAttribute("class"));
  if (item.originalWidth) clone.setAttribute("width", item.originalWidth); else clone.removeAttribute("width");
  if (item.originalHeight) clone.setAttribute("height", item.originalHeight); else clone.removeAttribute("height");
  clone.removeAttribute("role");
  return new XMLSerializer().serializeToString(clone);
}

function updateResultMeta(item = selected()) {
  if (!item || item.stage !== "done") { resultMeta.textContent = ""; return; }
  const suffix = item.saveError ? t("saveFailed") : item.dirty ? t("unsaved") : "";
  resultMeta.textContent = `${item.pathCount ?? 0} ${t("paths")} · ${item.outputName || "SVG"}${suffix ? ` · ${suffix}` : ""}`;
}

function syncEditorUi() {
  const item = selected();
  const editable = item?.stage === "done" && Boolean(activeSvg);
  viewerToolbar.hidden = !editable;
  editSection.hidden = !editable;
  statusStrip.hidden = editable;
  const hasSelection = editable && Boolean(selectedShape?.isConnected);
  selectionLabel.textContent = hasSelection && activeSvg && selectedShape
    ? t("shapeSelected", { count: [...activeSvg.querySelectorAll(EDITABLE_SELECTOR)].indexOf(selectedShape) + 1 })
    : t("noSelection");
  [fillColor, strokeColor, removeFill, removeStroke, deleteShape].forEach((control) => control.disabled = !hasSelection);
  undoEdit.disabled = !item?.undoStack?.length;
  redoEdit.disabled = !item?.redoStack?.length;
  saveEdits.disabled = !item?.dirty || !item.outputPath;
  saveEdits.classList.toggle("dirty", Boolean(item?.dirty));
  if (hasSelection && selectedShape) {
    const computed = getComputedStyle(selectedShape);
    const fill = selectedShape.style.fill || selectedShape.getAttribute("fill") || computed.fill;
    const stroke = selectedShape.style.stroke || selectedShape.getAttribute("stroke") || computed.stroke;
    fillColor.value = colorToHex(fill, "#315fce");
    strokeColor.value = colorToHex(stroke, "#252c39");
    removeFill.classList.toggle("active", fill === "none" || computed.fill === "none");
    removeStroke.classList.toggle("active", stroke === "none" || computed.stroke === "none");
  } else {
    removeFill.classList.remove("active");
    removeStroke.classList.remove("active");
  }
  updateResultMeta(item);
}

function selectShape(shape?: SVGGraphicsElement) {
  finishPathAnimations();
  selectedShape?.classList.remove("vector-selected");
  selectedShape = shape?.isConnected ? shape : undefined;
  selectedShape?.classList.add("vector-selected");
  syncEditorUi();
}

function pushUndo(item: Item) {
  item.undoStack ||= [];
  item.undoStack.push(serializeActiveSvg(item));
  if (item.undoStack.length > 50) item.undoStack.shift();
  item.redoStack = [];
}

function commitLiveEdit(item: Item) {
  item.svgText = serializeActiveSvg(item);
  item.dirty = item.svgText !== item.savedSvgText;
  item.saveError = false;
  item.pathCount = activeSvg?.querySelectorAll("path").length || 0;
  syncEditorUi();
}

function applyPaint(property: "fill" | "stroke", value: string) {
  const item = selected();
  if (!item || !selectedShape) return;
  pushUndo(item);
  selectedShape.style.setProperty(property, value);
  commitLiveEdit(item);
}

async function restoreEdit(item: Item, svg: string) {
  item.svgText = svg;
  item.dirty = svg !== item.savedSvgText;
  item.saveError = false;
  renderedOutput = "";
  await showItem(item, false);
}

async function undoCurrentEdit() {
  const item = selected();
  const previous = item?.undoStack?.pop();
  if (!item || !previous) return;
  item.redoStack ||= [];
  item.redoStack.push(serializeActiveSvg(item));
  await restoreEdit(item, previous);
}

async function redoCurrentEdit() {
  const item = selected();
  const next = item?.redoStack?.pop();
  if (!item || !next) return;
  item.undoStack ||= [];
  item.undoStack.push(serializeActiveSvg(item));
  await restoreEdit(item, next);
}

async function saveCurrentEdits() {
  const item = selected();
  if (!item?.dirty || !item.outputPath) return;
  const svg = serializeActiveSvg(item);
  saveEdits.classList.add("saving");
  saveEdits.disabled = true;
  try {
    await invoke("save_svg_edits", { path: item.outputPath, svg });
    item.svgText = svg;
    item.savedSvgText = svg;
    item.dirty = false;
    item.saveError = false;
  } catch {
    item.saveError = true;
  } finally {
    saveEdits.classList.remove("saving");
    syncEditorUi();
  }
}

function clearSvgWorkspace() {
  finishPathAnimations();
  activeSvg = undefined;
  activeViewport = undefined;
  viewBaseWidth = 0;
  viewBaseHeight = 0;
  selectedShape = undefined;
  viewerToolbar.hidden = true;
  editSection.hidden = true;
  statusStrip.hidden = false;
  artboard.classList.remove("is-panning");
}

function stopDepthPreview() {
  depthPreviewVersion += 1;
  if (depthAnimationFrame) cancelAnimationFrame(depthAnimationFrame);
  depthAnimationFrame = 0;
}

function fitArtboardElement(element: HTMLElement | SVGSVGElement, ratio: number) {
  artboardResizeObserver?.disconnect();
  const fit = () => {
    const maxWidth = artboard.clientWidth * 0.88;
    const maxHeight = artboard.clientHeight * 0.82;
    const fittedWidth = Math.min(maxWidth, maxHeight * ratio);
    element.style.width = `${fittedWidth}px`;
    element.style.height = `${fittedWidth / ratio}px`;
  };
  artboardResizeObserver = new ResizeObserver(fit);
  artboardResizeObserver.observe(artboard);
  fit();
}

function fitSvgViewport(element: HTMLElement, ratio: number) {
  artboardResizeObserver?.disconnect();
  const fit = () => {
    const maxWidth = artboard.clientWidth * 0.88;
    const maxHeight = artboard.clientHeight * 0.82;
    viewBaseWidth = Math.min(maxWidth, maxHeight * ratio);
    viewBaseHeight = viewBaseWidth / ratio;
    applyViewTransform();
  };
  activeViewport = element;
  artboardResizeObserver = new ResizeObserver(fit);
  artboardResizeObserver.observe(artboard);
  fit();
}

function loadImage(url: string) {
  return new Promise<HTMLImageElement>((resolve, reject) => {
    const image = new Image();
    image.onload = () => resolve(image);
    image.onerror = () => reject(new Error("Image preview could not be loaded"));
    image.src = url;
  });
}

async function showDepthSeparation(item: Item) {
  if (!item.sourceUrl || !item.depthUrl) return;
  const version = ++depthPreviewVersion;
  if (depthAnimationFrame) cancelAnimationFrame(depthAnimationFrame);
  const [source, depth] = await Promise.all([loadImage(item.sourceUrl), loadImage(item.depthUrl)]);
  if (version !== depthPreviewVersion || selectedId !== item.id || !busy(item)) return;

  const scale = Math.min(1, 720 / Math.max(source.naturalWidth, source.naturalHeight));
  const width = Math.max(1, Math.round(source.naturalWidth * scale));
  const height = Math.max(1, Math.round(source.naturalHeight * scale));
  const sourceCanvas = document.createElement("canvas");
  const depthCanvas = document.createElement("canvas");
  sourceCanvas.width = depthCanvas.width = width;
  sourceCanvas.height = depthCanvas.height = height;
  const sourceContext = sourceCanvas.getContext("2d", { willReadFrequently: true })!;
  const depthContext = depthCanvas.getContext("2d", { willReadFrequently: true })!;
  sourceContext.drawImage(source, 0, 0, width, height);
  depthContext.drawImage(depth, 0, 0, width, height);
  const sourcePixels = sourceContext.getImageData(0, 0, width, height);
  const depthPixels = depthContext.getImageData(0, 0, width, height).data;
  const binCount = 6;
  const buffers = Array.from({ length: binCount }, () => new Uint8ClampedArray(sourcePixels.data.length));
  for (let offset = 0; offset < sourcePixels.data.length; offset += 4) {
    const bin = Math.min(binCount - 1, Math.floor(depthPixels[offset] / 256 * binCount));
    buffers[bin].set(sourcePixels.data.subarray(offset, offset + 4), offset);
  }
  const layers = buffers.map((buffer) => {
    const layer = document.createElement("canvas");
    layer.width = width; layer.height = height;
    layer.getContext("2d")!.putImageData(new ImageData(buffer, width, height), 0, 0);
    return layer;
  });
  if (version !== depthPreviewVersion || selectedId !== item.id || !busy(item)) return;

  const canvas = document.createElement("canvas");
  canvas.className = "depth-separation-preview";
  canvas.width = width; canvas.height = height;
  canvas.setAttribute("role", "img");
  artboard.replaceChildren(canvas);
  fitArtboardElement(canvas, width / height);
  renderedOutput = `depth:${item.id}:${item.previewPath}`;
  const context = canvas.getContext("2d")!;
  const reducedMotion = matchMedia("(prefers-reduced-motion: reduce)").matches;
  const started = performance.now();
  const draw = (now: number) => {
    if (version !== depthPreviewVersion || selectedId !== item.id || !busy(item)) return;
    const pulse = reducedMotion ? 0.58 : 0.48 + Math.sin((now - started) / 760) * 0.24;
    const spread = Math.min(width, height) * 0.065 * pulse;
    context.clearRect(0, 0, width, height);
    layers.forEach((layer, index) => {
      const depthPosition = index / (binCount - 1) - 0.5;
      const offsetX = depthPosition * spread * 1.7;
      const offsetY = -depthPosition * spread * 0.48;
      const layerScale = 1 + depthPosition * 0.035 * pulse;
      context.save();
      context.translate(width / 2 + offsetX, height / 2 + offsetY);
      context.scale(layerScale, layerScale);
      context.shadowColor = "rgba(22, 31, 48, 0.28)";
      context.shadowBlur = Math.abs(depthPosition) * 16 * pulse;
      context.drawImage(layer, -width / 2, -height / 2);
      context.restore();
    });
    if (!reducedMotion) depthAnimationFrame = requestAnimationFrame(draw);
  };
  draw(performance.now());
}

async function showItem(item?: Item, animateSvg = true) {
  if (!item) return;
  const version = ++displayVersion;
  const isCurrent = () => version === displayVersion && selectedId === item.id;
  sourceName.textContent = item.name;
  sourceMeta.textContent = item.model === "detail" ? t("detail") : t("simple");
  const source = await loadSource(item).catch(() => "");
  if (!isCurrent()) return;
  sourceThumb.innerHTML = source ? `<img src="${source}" alt="" />` : I.image;
  if (item.stage === "done" && item.outputPath) {
    if (!item.svgText) {
      const asset = await invoke<Asset>("read_asset", { path: item.outputPath });
      item.svgText = asset.text;
    }
    if (!isCurrent()) return;
    if (item.svgText && renderedOutput !== item.outputPath) {
      stopDepthPreview();
      const svg = sanitizeSvg(item.svgText);
      if (item.originalWidth === undefined) item.originalWidth = svg.getAttribute("width") || "";
      if (item.originalHeight === undefined) item.originalHeight = svg.getAttribute("height") || "";
      const viewBox = (svg.getAttribute("viewBox") || "").trim().split(/[\s,]+/).map(Number);
      const width = viewBox.length === 4 && viewBox[2] > 0 ? viewBox[2] : Number.parseFloat(svg.getAttribute("width") || "");
      const height = viewBox.length === 4 && viewBox[3] > 0 ? viewBox[3] : Number.parseFloat(svg.getAttribute("height") || "");
      svg.removeAttribute("width"); svg.removeAttribute("height");
      svg.setAttribute("preserveAspectRatio", "xMidYMid meet");
      svg.setAttribute("role", "img");
      const viewport = document.createElement("div");
      viewport.className = "svg-viewport";
      viewport.append(svg);
      artboardResizeObserver?.disconnect();
      artboard.replaceChildren(viewport);
      activeSvg = svg;
      activeViewport = viewport;
      selectedShape = undefined;
      const ratio = Number.isFinite(width) && Number.isFinite(height) && width > 0 && height > 0 ? width / height : 1;
      fitSvgViewport(viewport, ratio);
      resetView();
      syncCanvasModes();
      if (item.savedSvgText === undefined) {
        item.savedSvgText = serializeActiveSvg(item);
        item.svgText = item.savedSvgText;
      }
      item.pathCount = svg.querySelectorAll("path").length;
      renderedOutput = item.outputPath;
      syncEditorUi();
      if (animateSvg) requestAnimationFrame(() => animatePaths(svg));
    }
  } else if (source && item.depthUrl && busy(item)) {
    clearSvgWorkspace();
    if (renderedOutput !== `depth:${item.id}:${item.previewPath}`) await showDepthSeparation(item);
  } else if (source && renderedOutput !== `source:${item.id}`) {
    clearSvgWorkspace();
    stopDepthPreview();
    artboardResizeObserver?.disconnect();
    artboard.innerHTML = `<img class="source-preview" src="${source}" alt="" />`;
    renderedOutput = `source:${item.id}`;
  }
}

function renderQueue() {
  queueList.replaceChildren();
  if (!items.length) {
    const empty = document.createElement("div"); empty.className = "queue-empty"; empty.textContent = t("emptyQueue");
    queueList.append(empty); return;
  }
  for (const item of items) {
    const row = document.createElement("div"); row.className = `queue-item ${item.id === selectedId ? "selected" : ""}`;
    const main = document.createElement("div"); main.className = "queue-item-main"; main.tabIndex = 0; main.setAttribute("role", "button");
    const thumb = document.createElement("span"); thumb.className = "queue-thumb";
    thumb.innerHTML = item.sourceUrl ? `<img src="${item.sourceUrl}" alt=""/>` : I.image;
    const copy = document.createElement("span"); copy.className = "queue-copy";
    const strong = document.createElement("strong"); strong.textContent = item.outputName || item.name;
    const small = document.createElement("small"); small.textContent = item.historyId ? t("savedResult") : stageLabel(item.stage);
    if (renamingId === item.id && item.historyId) {
      const input = document.createElement("input"); input.className = "queue-rename-input";
      input.value = (item.outputName || item.name).replace(/\.svg$/i, ""); input.setAttribute("aria-label", t("renameResult"));
      input.addEventListener("click", (event) => event.stopPropagation());
      input.addEventListener("keydown", (event) => {
        event.stopPropagation();
        if (event.key === "Enter") void renameHistoryItem(item, input.value);
        else if (event.key === "Escape") { renamingId = ""; render(); }
      });
      input.addEventListener("blur", () => window.setTimeout(() => {
        if (renamingId === item.id) { renamingId = ""; render(); }
      }, 80));
      copy.append(input, small); window.setTimeout(() => { input.focus(); input.select(); });
    } else copy.append(strong, small);
    main.append(thumb, copy);
    main.addEventListener("click", () => {
      clearSvgWorkspace(); selectedId = item.id; renderedOutput = ""; render(); void showItem(item);
    });
    main.addEventListener("keydown", (event) => {
      if (event.key === "Enter" || event.key === " ") { event.preventDefault(); main.click(); }
    });
    const stateDot = document.createElement("i"); stateDot.className = `queue-state state ${item.stage}`;
    const actions = document.createElement("span"); actions.className = "queue-actions";
    if (item.historyId) {
      const rename = document.createElement("button"); rename.type = "button"; rename.innerHTML = I.rename;
      rename.title = t("renameResult"); rename.setAttribute("aria-label", t("renameResult"));
      rename.addEventListener("click", () => { renamingId = item.id; renderQueue(); });
      const remove = document.createElement("button"); remove.type = "button"; remove.className = "danger"; remove.innerHTML = I.trash;
      remove.title = t("deleteResult"); remove.setAttribute("aria-label", t("deleteResult"));
      remove.addEventListener("click", () => void deleteHistoryItem(item));
      actions.append(rename, remove);
    }
    row.append(main, stateDot, actions); queueList.append(row);
  }
}

function render() {
  if (!renamingId) renderQueue();
  const item = selected();
  q<HTMLButtonElement>("#generate").disabled = !item || item.stage !== "draft";
  q<HTMLButtonElement>("#cancel").hidden = !item || !busy(item);
  q<HTMLButtonElement>("#openFolder").hidden = !item?.outputPath;
  document.querySelectorAll<HTMLButtonElement>("[data-model]").forEach((button) => {
    const model = button.dataset.model as Model;
    button.classList.toggle("active", (item?.model || defaultModel) === model);
    button.disabled = !!item && item.stage !== "draft";
  });
  if (item) {
    statusTitle.textContent = stageLabel(item.stage);
    statusDetail.textContent = localizedProgress(item);
    updateResultMeta(item);
  }
  if (!item || item.stage !== "done") { viewerToolbar.hidden = true; editSection.hidden = true; }
  updateProgressUi();
}

async function renameHistoryItem(item: Item, newName: string) {
  if (!item.historyId) return;
  try {
    const entry = await invoke<HistoryEntry>("rename_history_result", { id: item.historyId, newName });
    item.outputPath = entry.outputPath; item.outputName = entry.outputName;
    if (item.id === selectedId) renderedOutput = entry.outputPath;
    renamingId = ""; render();
  } catch (error) {
    window.alert(`${t("renameFailed")}: ${String(error)}`);
    renamingId = ""; render();
  }
}

async function deleteHistoryItem(item: Item) {
  if (!item.historyId || !window.confirm(t("deleteResultConfirm"))) return;
  try {
    await invoke("delete_history_result", { id: item.historyId });
    const index = items.indexOf(item); items.splice(index, 1);
    if (selectedId === item.id) {
      clearSvgWorkspace(); renderedOutput = ""; selectedId = items[Math.min(index, items.length - 1)]?.id || "";
      if (!selectedId) artboard.innerHTML = `<div class="empty-state">${I.vector}<strong>${t("canvasEmpty")}</strong><span>${t("canvasHint")}</span></div>`;
    }
    render(); if (selected()) await showItem(selected());
  } catch (error) {
    window.alert(`${t("deleteFailed")}: ${String(error)}`);
  }
}

function comparablePath(path?: string) { return (path || "").toLowerCase(); }

async function refreshHistory() {
  if (historyRefreshing || !window.invoke) return;
  historyRefreshing = true;
  try {
    const entries = await invoke<HistoryEntry[]>("history_results");
    const validIds = new Set(entries.map((entry) => entry.id));
    const validPaths = new Set(entries.map((entry) => comparablePath(entry.outputPath)));
    for (const entry of entries) {
      let item = items.find((candidate) => candidate.historyId === entry.id)
        || items.find((candidate) => comparablePath(candidate.outputPath) === comparablePath(entry.outputPath));
      if (item) {
        item.historyId = entry.id; item.createdAtMs = entry.createdAtMs;
        item.outputPath = entry.outputPath; item.outputName = entry.outputName;
        continue;
      }
      const model = entry.metadata?.model === "detail" ? "detail" : "simple";
      item = {
        id: `history_${entry.id}`, batchId: `history_${entry.id}`, path: entry.sourcePath,
        name: basename(entry.sourcePath || entry.outputName), model, outputDir: entry.outputPath.replace(/[\\/][^\\/]+$/, ""),
        stage: "done", outputPath: entry.outputPath, outputName: entry.outputName,
        historyId: entry.id, createdAtMs: entry.createdAtMs,
      };
      try { item.sourceUrl = (await invoke<Asset>("read_asset", { path: entry.sourcePath })).dataUrl; } catch { /* Source thumbnails are optional. */ }
      items.push(item);
    }
    const selectedBefore = selectedId;
    items = items.filter((item) => {
      if (item.historyId) return validIds.has(item.historyId);
      return item.stage !== "done" || !item.outputPath || validPaths.has(comparablePath(item.outputPath));
    });
    if (!items.some((item) => item.id === selectedId)) selectedId = items[0]?.id || "";
    render();
    if (selectedId && selectedId !== selectedBefore) { renderedOutput = ""; await showItem(selected()); }
  } catch { /* The active queue remains available if history storage is unavailable. */ }
  finally { historyRefreshing = false; }
}

async function restoreCurrentJobs() {
  try {
    const statuses = await invoke<JobStatus[]>("job_statuses");
    for (const status of statuses.filter((value) => ["preparing", "generating", "finalizing"].includes(value.stage))) {
      if (items.some((item) => item.jobId === status.jobId)) continue;
      const item: Item = {
        id: `recovered_${status.jobId}`, batchId: `recovered_${status.jobId}`, path: status.sourceImagePath,
        name: basename(status.sourceImagePath), model: status.model, outputDir, stage: status.stage, jobId: status.jobId,
        progress: status.progressRatio, progressText: status.progressText, progressKey: status.progressKey, phase: status.phase,
        previewPath: status.previewPath, operationStartedAt: Date.now() - Math.max(0, status.elapsedMs || 0),
        estimatedTotalMs: status.estimatedTotalMs,
      };
      try { item.sourceUrl = (await invoke<Asset>("read_asset", { path: item.path })).dataUrl; } catch { /* Source may have moved. */ }
      items.push(item);
    }
    if (!selectedId && items.length) selectedId = items[0].id;
  } catch { /* No running jobs to recover. */ }
}

async function addImagePaths(paths: string[]) {
  if (!paths.length) return;
  const existing = new Set(items.map((item) => item.path.toLowerCase()));
  paths = paths.filter((path) => {
    const key = path.toLowerCase();
    if (existing.has(key)) return false;
    existing.add(key);
    return true;
  });
  if (!paths.length) return;
  const batchId = `batch-${Date.now()}`;
  const created = paths.map((path, index): Item => ({
    id: `${batchId}-${index}`, batchId, path, name: basename(path), model: defaultModel,
    outputDir, stage: "draft",
  }));
  items.push(...created); selectedId = created[0].id; renderedOutput = "";
  await Promise.all(created.map((item) => loadSource(item).catch(() => "")));
  render(); await showItem(selected());
}

async function addImages() { await addImagePaths(await invoke<string[]>("pick_images")); }

async function loadDepthFor(item: Item, path: string) {
  if (!path || item.previewPath === path && item.depthUrl) return;
  item.previewPath = path;
  try {
    item.depthUrl = (await invoke<Asset>("read_asset", { path })).dataUrl;
    if (item.id === selectedId && busy(item)) {
      renderedOutput = "";
      await showItem(item);
    }
  } catch { item.depthUrl = ""; }
}

async function pump() {
  if (pumping) return;
  pumping = true;
  try {
    while (items.filter(busy).length < 2) {
      const item = items.find((value) => value.stage === "queued");
      if (!item) break;
      try {
        const status = await invoke<JobStatus>("start_job", { imagePath: item.path, outputDir: item.outputDir, model: item.model });
        item.jobId = status.jobId; item.stage = status.stage; item.progressText = status.progressText;
        item.progressKey = status.progressKey; item.phase = status.phase;
        beginProgress(item, status);
        if (status.previewPath) void loadDepthFor(item, status.previewPath);
      } catch (error) {
        item.stage = "failed"; item.error = error instanceof Error ? error.message : String(error);
      }
      render();
    }
  } finally { pumping = false; }
}

async function poll() {
  try {
    const statuses = await invoke<JobStatus[]>("job_statuses");
    for (const status of statuses) {
      const item = items.find((value) => value.jobId === status.jobId);
      if (!item) continue;
      const changed = item.stage !== status.stage;
      item.stage = status.stage; item.progress = status.progressRatio; item.progressText = status.progressText;
      item.outputPath = status.outputPath; item.outputName = status.outputName; item.error = status.error;
      item.progressKey = status.progressKey; item.phase = status.phase;
      if (busy(item)) {
        if (!item.operationStartedAt) beginProgress(item, status);
        if (status.estimatedTotalMs) item.estimatedTotalMs = status.estimatedTotalMs;
      }
      if (status.previewPath) void loadDepthFor(item, status.previewPath);
      if (changed && item.id === selectedId) { renderedOutput = ""; void showItem(item); }
      if (changed && status.stage === "done") void refreshHistory();
    }
    render(); void pump();
  } catch { /* Host may be closing. */ }
}

q("#addImages").addEventListener("click", () => void addImages());
q("#chooseImages").addEventListener("click", () => void addImages());
q("#chooseFolder").addEventListener("click", async () => {
  const chosen = await invoke<string | null>("pick_output_dir");
  if (!chosen) return;
  outputDir = chosen; folderPath.textContent = chosen;
  const item = selected();
  if (item?.stage === "draft") items.filter((value) => value.batchId === item.batchId && value.stage === "draft").forEach((value) => value.outputDir = chosen);
});
q("#generate").addEventListener("click", () => {
  const item = selected(); if (!item || item.stage !== "draft") return;
  items.filter((value) => value.batchId === item.batchId && value.stage === "draft").forEach((value) => value.stage = "queued");
  render(); void pump();
});
q("#cancel").addEventListener("click", async () => { const item = selected(); if (item?.jobId) await invoke("cancel_job", { jobId: item.jobId }); void poll(); });
q("#openFolder").addEventListener("click", () => { const item = selected(); void invoke("open_output", { path: item?.outputPath || outputDir }); });
document.querySelectorAll<HTMLButtonElement>("[data-model]").forEach((button) => button.onclick = () => {
  const model = button.dataset.model as Model; defaultModel = model;
  const item = selected();
  if (item?.stage === "draft") items.filter((value) => value.batchId === item.batchId && value.stage === "draft").forEach((value) => value.model = model);
  render();
});
q("#zoomOut").addEventListener("click", () => setZoom(viewScale / 1.2));
q("#zoomIn").addEventListener("click", () => setZoom(viewScale * 1.2));
q("#fitView").addEventListener("click", resetView);
q("#canvasBackground").addEventListener("click", () => { backgroundMode = (backgroundMode + 1) % 3; syncCanvasModes(); });
q("#showOutlines").addEventListener("click", () => { outlineVisible = !outlineVisible; syncCanvasModes(); });
fillColor.addEventListener("change", () => applyPaint("fill", fillColor.value));
strokeColor.addEventListener("change", () => applyPaint("stroke", strokeColor.value));
removeFill.addEventListener("click", () => applyPaint("fill", "none"));
removeStroke.addEventListener("click", () => applyPaint("stroke", "none"));
undoEdit.addEventListener("click", () => void undoCurrentEdit());
redoEdit.addEventListener("click", () => void redoCurrentEdit());
deleteShape.addEventListener("click", () => {
  const item = selected();
  if (!item || !selectedShape) return;
  pushUndo(item);
  selectedShape.remove();
  selectedShape = undefined;
  commitLiveEdit(item);
});
saveEdits.addEventListener("click", () => void saveCurrentEdits());

artboard.addEventListener("wheel", (event) => {
  if (!activeViewport) return;
  event.preventDefault();
  const bounds = artboard.getBoundingClientRect();
  const anchor = { x: event.clientX - bounds.left - bounds.width / 2, y: event.clientY - bounds.top - bounds.height / 2 };
  setZoom(viewScale * (event.deltaY < 0 ? 1.12 : 0.89), anchor);
}, { passive: false });
artboard.addEventListener("dblclick", (event) => {
  if (!activeViewport || (event.target as Element).closest(EDITABLE_SELECTOR)) return;
  resetView();
});
artboard.addEventListener("pointerdown", (event) => {
  if (!activeViewport || event.button !== 0) return;
  panStart = { x: event.clientX, y: event.clientY, viewX, viewY };
  panMoved = false;
});
artboard.addEventListener("pointermove", (event) => {
  if (!panStart) return;
  const dx = event.clientX - panStart.x;
  const dy = event.clientY - panStart.y;
  if (Math.abs(dx) + Math.abs(dy) > 3 && !panMoved) {
    panMoved = true;
    artboard.setPointerCapture(event.pointerId);
  }
  if (!panMoved) return;
  viewX = panStart.viewX + dx;
  viewY = panStart.viewY + dy;
  artboard.classList.add("is-panning");
  applyViewTransform();
});
artboard.addEventListener("pointerup", (event) => {
  if (!panStart) return;
  panStart = undefined;
  artboard.classList.remove("is-panning");
  if (artboard.hasPointerCapture(event.pointerId)) artboard.releasePointerCapture(event.pointerId);
  if (panMoved) setTimeout(() => { panMoved = false; }, 0);
});
artboard.addEventListener("pointercancel", () => {
  panStart = undefined;
  panMoved = false;
  artboard.classList.remove("is-panning");
});
artboard.addEventListener("click", (event) => {
  if (panMoved || !activeSvg) return;
  const target = (event.target as Element).closest(EDITABLE_SELECTOR) as SVGGraphicsElement | null;
  selectShape(target && activeSvg.contains(target) ? target : undefined);
});

window.addEventListener("keydown", (event) => {
  if (event.target instanceof Element && event.target.closest("input")) return;
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "s") {
    event.preventDefault();
    void saveCurrentEdits();
  } else if ((event.ctrlKey || event.metaKey) && event.key === "0") {
    event.preventDefault();
    resetView();
  } else if ((event.ctrlKey || event.metaKey) && (event.key === "+" || event.key === "=")) {
    event.preventDefault();
    setZoom(viewScale * 1.2);
  } else if ((event.ctrlKey || event.metaKey) && event.key === "-") {
    event.preventDefault();
    setZoom(viewScale / 1.2);
  } else if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "z") {
    event.preventDefault();
    void (event.shiftKey ? redoCurrentEdit() : undoCurrentEdit());
  } else if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "y") {
    event.preventDefault();
    void redoCurrentEdit();
  } else if ((event.key === "Delete" || event.key === "Backspace") && selectedShape) {
    event.preventDefault();
    deleteShape.click();
  } else if (event.key === "Escape" && selectedShape) {
    selectShape();
  }
});
q("#minimize").addEventListener("click", () => void invoke("minimize_window"));
q("#close").addEventListener("click", () => void invoke("close_window"));
q("#dragRegion").addEventListener("mousedown", (event) => { if (!(event.target as Element).closest("button")) void invoke("start_drag"); });

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
  const demo = import.meta.env.DEV && new URLSearchParams(location.search).has("demo");
  if (demo) {
    const svg = `<svg viewBox="0 0 640 480" xmlns="http://www.w3.org/2000/svg"><path fill="#edf2ff" d="M70 60h500v360H70z"/><path fill="#315fce" d="M112 120h182v96H112z"/><path fill="#ff7b6b" d="M330 120h198v44H330z"/><path fill="#55cda7" d="M330 184h140v32H330z"/><path fill="#252c39" d="M112 252h416v24H112z"/><path fill="#8da8ef" d="M112 300h310v20H112z"/><path fill="#cad4e7" d="M112 340h370v20H112z"/></svg>`;
    const demo: Item = { id: "demo", batchId: "demo", path: "sample.png", name: "sample.png", model: "simple", outputDir, stage: "done", outputPath: "demo.svg", outputName: "sample.svg", svgText: svg, historyId: pageParams.has("history") ? "demo-history" : undefined };
    items = [demo]; selectedId = demo.id;
  } else if (window.invoke) {
    await restoreCurrentJobs();
    await refreshHistory();
    window.setInterval(() => void refreshHistory(), 5000);
  }
  const updateReady = async () => {
    const status = await invoke<string>("runtime_preparation_status").catch(() => "preparing");
    const badge = q<HTMLElement>("#readiness");
    badge.className = `readiness ${status === "ready" ? "" : "busy"}`;
    q("#readyText").textContent = status === "ready" ? t("ready") : status === "partial" ? t("oneWorker") : t("preparing");
  };
  void invoke("prepare_runtime"); void updateReady(); setInterval(updateReady, 2500); setInterval(poll, 700);
  setInterval(updateProgressUi, 250);
  render(); await showItem(selected());
  if (demo && selected()) {
    const demoZoom = Number(pageParams.get("zoom"));
    if (Number.isFinite(demoZoom) && demoZoom > 0) setZoom(demoZoom);
    if (pageParams.has("selected")) selectShape(activeSvg?.querySelector<SVGGraphicsElement>(EDITABLE_SELECTOR) || undefined);
  }
}
void boot();
window.addEventListener("focus", () => void refreshHistory());
