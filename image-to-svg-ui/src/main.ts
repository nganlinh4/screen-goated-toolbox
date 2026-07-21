import "./styles.css";
import { setLanguage, t } from "./i18n";

type Model = "simple" | "detail";
type Stage = "draft" | "queued" | "preparing" | "generating" | "finalizing" | "done" | "failed" | "cancelled";
type HostContext = { theme?: "light" | "dark"; language?: string };
type Asset = { dataUrl?: string; text?: string; sizeBytes: number };
type JobStatus = {
  jobId: string; stage: Stage; progressText: string; elapsedMs?: number; estimatedTotalMs?: number;
  progressRatio?: number; outputPath?: string; outputName?: string; sourceImagePath: string;
  model: Model; creditsRemaining?: number; error?: string;
};
type Item = {
  id: string; batchId: string; path: string; name: string; model: Model; outputDir: string;
  stage: Stage; jobId?: string; progress?: number; progressText?: string; outputPath?: string;
  outputName?: string; error?: string; sourceUrl?: string; svgText?: string; pathCount?: number;
};

declare global {
  interface Window {
    invoke?: <T = unknown>(cmd: string, args?: unknown) => Promise<T>;
    __SGT_CONTEXT__?: HostContext;
    applyHostContext?: (context: HostContext) => void;
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
};

const app = document.querySelector<HTMLElement>("#app")!;
app.innerHTML = `
<section class="shell">
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
        <div class="status-strip" id="statusStrip"><span class="status-icon">${I.sparkle}</span><span><strong id="statusTitle">${t("selectJob")}</strong><small id="statusDetail"></small></span><i class="progress"><b id="progressFill"></b></i></div>
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
const progressFill = q<HTMLElement>("#progressFill");
const resultMeta = q<HTMLElement>("#resultMeta");

let items: Item[] = [];
let selectedId = "";
let outputDir = "";
let defaultModel: Model = "simple";
let pumping = false;
let renderedOutput = "";

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

function animatePaths(svg: SVGSVGElement) {
  if (matchMedia("(prefers-reduced-motion: reduce)").matches) return;
  const paths = [...svg.querySelectorAll<SVGPathElement>("path")].slice(0, 180);
  paths.forEach((path, index) => {
    let length = 0;
    try { length = path.getTotalLength(); } catch { return; }
    if (!Number.isFinite(length) || length <= 0) return;
    const computed = getComputedStyle(path);
    const originalStroke = path.style.stroke;
    const originalFillOpacity = path.style.fillOpacity;
    path.style.stroke = computed.stroke === "none" ? "var(--ink-accent)" : computed.stroke;
    path.style.strokeDasharray = `${length}`;
    path.style.strokeDashoffset = `${length}`;
    path.style.fillOpacity = "0";
    const animation = path.animate(
      [{ strokeDashoffset: length, fillOpacity: 0 }, { strokeDashoffset: 0, fillOpacity: 0, offset: 0.78 }, { strokeDashoffset: 0, fillOpacity: 1 }],
      { duration: 780 + Math.min(length, 520), delay: index * 32, easing: "cubic-bezier(.2,.75,.25,1)", fill: "forwards" },
    );
    animation.finished.then(() => {
      path.style.stroke = originalStroke;
      path.style.fillOpacity = originalFillOpacity;
      path.style.strokeDasharray = "";
      path.style.strokeDashoffset = "";
    }).catch(() => undefined);
  });
}

async function showItem(item?: Item) {
  if (!item) return;
  sourceName.textContent = item.name;
  sourceMeta.textContent = item.model === "detail" ? t("detail") : t("simple");
  const source = await loadSource(item).catch(() => "");
  sourceThumb.innerHTML = source ? `<img src="${source}" alt="" />` : I.image;
  if (item.stage === "done" && item.outputPath) {
    if (!item.svgText) {
      const asset = await invoke<Asset>("read_asset", { path: item.outputPath });
      item.svgText = asset.text;
    }
    if (item.svgText && renderedOutput !== item.outputPath) {
      const svg = sanitizeSvg(item.svgText);
      svg.removeAttribute("width"); svg.removeAttribute("height");
      svg.setAttribute("role", "img");
      artboard.replaceChildren(svg);
      item.pathCount = svg.querySelectorAll("path").length;
      resultMeta.textContent = `${item.pathCount} ${t("paths")} · ${item.outputName || "SVG"}`;
      renderedOutput = item.outputPath;
      requestAnimationFrame(() => animatePaths(svg));
    }
  } else if (source && renderedOutput !== `source:${item.id}`) {
    artboard.innerHTML = `<img class="source-preview" src="${source}" alt="" />`;
    renderedOutput = `source:${item.id}`;
  }
}

function render() {
  if (!items.length) queueList.innerHTML = `<div class="queue-empty">${t("emptyQueue")}</div>`;
  else queueList.innerHTML = items.map((item) => `
    <button class="queue-item ${item.id === selectedId ? "selected" : ""}" data-id="${item.id}">
      <span class="queue-thumb">${item.sourceUrl ? `<img src="${item.sourceUrl}" alt=""/>` : I.image}</span>
      <span><strong>${item.name}</strong><small>${stageLabel(item.stage)}</small></span>
      <i class="state ${item.stage}"></i>
    </button>`).join("");
  queueList.querySelectorAll<HTMLElement>("[data-id]").forEach((button) => button.onclick = () => {
    selectedId = button.dataset.id || ""; renderedOutput = ""; render(); void showItem(selected());
  });
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
    statusDetail.textContent = item.error || item.progressText || item.outputName || "";
    progressFill.style.width = `${Math.round((item.progress || (item.stage === "done" ? 1 : 0)) * 100)}%`;
    resultMeta.textContent = item.stage === "done" ? `${item.pathCount ?? ""} ${t("paths")} · ${item.outputName || "SVG"}` : "";
  }
}

async function addImages() {
  const paths = await invoke<string[]>("pick_images");
  if (!paths.length) return;
  const batchId = `batch-${Date.now()}`;
  const created = paths.map((path, index): Item => ({
    id: `${batchId}-${index}`, batchId, path, name: basename(path), model: defaultModel,
    outputDir, stage: "draft",
  }));
  items.push(...created); selectedId = created[0].id; renderedOutput = "";
  await Promise.all(created.map((item) => loadSource(item).catch(() => "")));
  render(); await showItem(created[0]);
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
      if (changed && item.id === selectedId) { renderedOutput = ""; void showItem(item); }
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
q("#minimize").addEventListener("click", () => void invoke("minimize_window"));
q("#close").addEventListener("click", () => void invoke("close_window"));
q("#dragRegion").addEventListener("mousedown", (event) => { if (!(event.target as Element).closest("button")) void invoke("start_drag"); });

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
  if (import.meta.env.DEV && new URLSearchParams(location.search).has("demo")) {
    const svg = `<svg viewBox="0 0 640 480" xmlns="http://www.w3.org/2000/svg"><path fill="#edf2ff" d="M70 60h500v360H70z"/><path fill="#315fce" d="M112 120h182v96H112z"/><path fill="#ff7b6b" d="M330 120h198v44H330z"/><path fill="#55cda7" d="M330 184h140v32H330z"/><path fill="#252c39" d="M112 252h416v24H112z"/><path fill="#8da8ef" d="M112 300h310v20H112z"/><path fill="#cad4e7" d="M112 340h370v20H112z"/></svg>`;
    const demo: Item = { id: "demo", batchId: "demo", path: "sample.png", name: "sample.png", model: "simple", outputDir, stage: "done", outputPath: "demo.svg", outputName: "sample.svg", svgText: svg };
    items = [demo]; selectedId = demo.id;
  }
  const updateReady = async () => {
    const status = await invoke<string>("runtime_preparation_status").catch(() => "preparing");
    const badge = q<HTMLElement>("#readiness");
    badge.className = `readiness ${status === "ready" ? "" : "busy"}`;
    q("#readyText").textContent = status === "ready" ? t("ready") : status === "partial" ? t("oneWorker") : t("preparing");
  };
  void invoke("prepare_runtime"); void updateReady(); setInterval(updateReady, 2500); setInterval(poll, 700);
  render(); void showItem(selected());
}
void boot();
