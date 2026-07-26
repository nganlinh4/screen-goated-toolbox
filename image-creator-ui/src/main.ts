import "./styles.css";
import { copyFor, type Copy } from "./i18n";
import { ICONS as I } from "./icons";
import {
  escapeHtml,
  historyReferences,
  jobReferences,
  MAX_REFERENCES,
  pathName,
  uniquePaths,
  type DialogState,
  type DraftSession,
  type HistoryEntry,
  type JobStatus,
  type Selection,
} from "./models";
import { guardPollRendering } from "./pollRenderGuard";
import { PreviewStore } from "./previewStore";
import { stageMarkup } from "./stageMarkup";

declare global {
  interface Window {
    invoke?: <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
    __SGT_CONTEXT__?: { theme?: string; language?: string };
    applyHostContext?: (context: { theme?: string; language?: string }) => void;
    handleNativeFileDrop?: (paths: string[]) => void;
    handleNativeFileDrag?: (active: boolean) => void;
  }
}

const context = window.__SGT_CONTEXT__ ?? {};
const app = document.querySelector<HTMLDivElement>("#app")!;
if (!app) throw new Error("App root is missing");

let copy: Copy = copyFor(context.language);
let drafts: DraftSession[] = [];
let jobs: JobStatus[] = [];
let history: HistoryEntry[] = [];
let selectedKey = "";
let outputDir = "";
let compare = 50;
let message = "";
let preparationStatus = "preparing";
let dialog: DialogState | null = null;
let renderVersion = 0;
let sessionSequence = 0;
let fitObserver: ResizeObserver | undefined;
let hydrationQueue = Promise.resolve();

async function invoke<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  if (!window.invoke) throw new Error("Native host is unavailable");
  return window.invoke<T>(command, args);
}

const previews = new PreviewStore(invoke);
const renderAfterPoll = guardPollRendering(app, render);

function terminal(stage: string): boolean {
  return stage === "done" || stage === "failed" || stage === "cancelled";
}

function busy(stage: string): boolean {
  return !terminal(stage) && stage !== "draft";
}

function publicStage(stage: string): string {
  return ["queued", "preparing", "uploading", "generating", "finalizing", "done", "failed", "cancelled"]
    .includes(stage) ? stage : "preparing";
}

function currentDraft(): DraftSession | undefined {
  return drafts.find((item) => item.key === selectedKey);
}

function referenceTitle(paths: string[]): string {
  if (paths.length === 0) return copy.newImage;
  if (paths.length === 1) return pathName(paths[0]);
  return copy.referenceCount(paths.length);
}

function selected(): Selection | null {
  const job = jobs.find((item) => item.jobId === selectedKey);
  if (job) {
    const references = jobReferences(job);
    return {
      key: job.jobId,
      kind: "job",
      referencePaths: references,
      output: job.outputPath,
      title: job.outputName || referenceTitle(references),
      prompt: job.prompt,
      width: job.width,
      height: job.height,
    };
  }
  const entry = history.find((item) => item.id === selectedKey);
  if (entry) {
    return {
      key: entry.id,
      kind: "history",
      referencePaths: historyReferences(entry),
      output: entry.outputPath,
      title: entry.outputName,
      prompt: entry.metadata?.prompt || "",
      width: entry.metadata?.width,
      height: entry.metadata?.height,
    };
  }
  const draft = currentDraft() ?? drafts[0];
  return draft ? {
    key: draft.key,
    kind: "draft",
    referencePaths: draft.referencePaths,
    title: referenceTitle(draft.referencePaths),
    prompt: draft.prompt,
  } : null;
}

function newSession(renderNow = true) {
  const draft: DraftSession = {
    key: `draft-${Date.now()}-${++sessionSequence}`,
    referencePaths: [],
    prompt: "",
  };
  drafts = [...drafts, draft];
  selectedKey = draft.key;
  message = "";
  if (renderNow) render();
}

function attachReferences(paths: string[]) {
  let draft = currentDraft();
  if (!draft) {
    newSession(false);
    draft = currentDraft();
  }
  if (!draft) return;
  const combined = uniquePaths([...draft.referencePaths, ...paths]);
  if (combined.length > MAX_REFERENCES) message = copy.referenceLimit(MAX_REFERENCES);
  else message = "";
  draft.referencePaths = combined.slice(0, MAX_REFERENCES);
  render();
}

async function pickImages() {
  attachReferences(await invoke<string[]>("pick_images"));
}

async function chooseOutput() {
  const next = await invoke<string | null>("pick_output_dir");
  if (next) outputDir = next;
  render();
}

async function submit() {
  const selection = selected();
  if (!selection) return;
  const selectedJob = jobs.find((item) => item.jobId === selectedKey);
  if (selectedJob && busy(selectedJob.stage)) return;
  const frozenPrompt = selection.prompt.trim();
  if (!frozenPrompt) {
    message = copy.promptRequired;
    render();
    return;
  }

  const draft = currentDraft();
  try {
    const status = await invoke<JobStatus>("start_job", {
      imagePaths: [...selection.referencePaths],
      outputDir,
      prompt: frozenPrompt,
    });
    jobs = [...jobs.filter((item) => item.jobId !== status.jobId), status];
    if (draft) drafts = drafts.filter((item) => item.key !== draft.key);
    selectedKey = status.jobId;
    message = "";
  } catch {
    message = copy.failed;
  }
  render();
}

async function refresh() {
  try {
    const [nextJobs, nextHistory] = await Promise.all([
      invoke<JobStatus[]>("job_statuses"),
      invoke<HistoryEntry[]>("history_results"),
    ]);
    jobs = nextJobs;
    history = nextHistory;
    const exists = jobs.some((item) => item.jobId === selectedKey)
      || drafts.some((item) => item.key === selectedKey)
      || history.some((item) => item.id === selectedKey);
    if (!exists) selectedKey = jobs[jobs.length - 1]?.jobId ?? drafts[0]?.key ?? history[0]?.id ?? "";
    renderAfterPoll();
  } catch {
    // A hidden or closing host can reject a poll; the next poll reconciles it.
  }
}

async function updateReady() {
  preparationStatus = await invoke<string>("runtime_preparation_status").catch(() => "preparing");
  renderAfterPoll();
}

async function cancel(jobId: string) {
  jobs = await invoke<JobStatus[]>("cancel_job", { jobId });
  render();
}

async function commitDialog() {
  if (!dialog) return;
  const current = dialog;
  dialog = null;
  try {
    if (current.kind === "rename") {
      const value = current.value.trim();
      if (value && value !== current.entry.outputName) {
        await invoke("rename_history_result", { id: current.entry.id, newName: value });
      }
    } else {
      await invoke("delete_history_result", { id: current.entry.id });
      if (selectedKey === current.entry.id) selectedKey = "";
    }
    await refresh();
  } catch {
    message = current.kind === "rename" ? copy.rename : copy.delete;
    render();
  }
}

function statusLabel(job: JobStatus): string {
  switch (publicStage(job.stage)) {
    case "queued": return copy.queued;
    case "preparing": return copy.preparing;
    case "uploading": return copy.uploading;
    case "generating": return copy.generating;
    case "finalizing": return copy.finalizing;
    case "done": return copy.ready;
    case "failed": return copy.failed;
    case "cancelled": return copy.cancelled;
    default: return copy.preparing;
  }
}

function thumb(path: string | undefined): string {
  return path ? ` data-thumb="${escapeHtml(path)}"` : "";
}

function renderQueue(): string {
  const knownOutputs = new Set(
    jobs.map((job) => job.outputPath?.toLocaleLowerCase()).filter((path): path is string => Boolean(path)),
  );
  const jobRows = jobs.slice().reverse().map((job) => {
    const references = jobReferences(job);
    const preview = job.outputPath || references[0];
    return `<div class="queue-item ${selectedKey === job.jobId ? "selected" : ""}">
      <button class="queue-item-main" type="button" data-select="${escapeHtml(job.jobId)}">
        <span class="queue-thumb"${thumb(preview)}>${I.image}</span>
        <span class="queue-copy"><strong>${escapeHtml(job.outputName || referenceTitle(references))}</strong>
          <small>${escapeHtml(statusLabel(job))}</small></span>
        <i class="queue-state state ${publicStage(job.stage)}"></i>
      </button>
    </div>`;
  }).join("");
  const draftRows = drafts.map((draft) => `
    <div class="queue-item ${selectedKey === draft.key ? "selected" : ""}">
      <button class="queue-item-main" type="button" data-select="${escapeHtml(draft.key)}">
        <span class="queue-thumb"${thumb(draft.referencePaths[0])}>${draft.referencePaths.length ? I.image : I.sparkle}</span>
        <span class="queue-copy"><strong>${escapeHtml(referenceTitle(draft.referencePaths))}</strong>
          <small>${draft.referencePaths.length ? copy.referenceReady : copy.noReferences}</small></span>
        <i class="queue-state state done"></i>
      </button>
      <span class="queue-actions"><button type="button" class="danger"
        data-remove-session="${escapeHtml(draft.key)}" title="${copy.delete}">${I.trash}</button></span>
    </div>`).join("");
  const historyRows = history
    .filter((entry) => !knownOutputs.has(entry.outputPath.toLocaleLowerCase()))
    .map((entry) => `
      <div class="queue-item ${selectedKey === entry.id ? "selected" : ""}">
        <button class="queue-item-main" type="button" data-select="${escapeHtml(entry.id)}">
          <span class="queue-thumb"${thumb(entry.outputPath)}>${I.image}</span>
          <span class="queue-copy"><strong>${escapeHtml(entry.outputName)}</strong>
            <small>${copy.savedResult}</small></span><i class="queue-state state done"></i>
        </button>
        <span class="queue-actions">
          <button type="button" data-rename="${escapeHtml(entry.id)}" title="${copy.rename}">${I.rename}</button>
          <button type="button" class="danger" data-delete="${escapeHtml(entry.id)}"
            title="${copy.delete}">${I.trash}</button>
        </span>
      </div>`).join("");
  return jobRows + draftRows + historyRows;
}

function renderReferences(selection: Selection | null, editable: boolean): string {
  const references = selection?.referencePaths ?? [];
  const list = references.length ? `<div class="reference-list">
    ${references.map((path, index) => `<div class="reference-chip">
      <span data-thumb="${escapeHtml(path)}">${I.image}</span>
      <small title="${escapeHtml(path)}">${escapeHtml(pathName(path))}</small>
      ${editable ? `<button type="button" data-remove-reference="${index}"
        title="${copy.removeReference}">${I.close}</button>` : ""}
    </div>`).join("")}
  </div>` : `<p class="reference-empty">${copy.noReferences}</p>`;
  const add = editable ? `<button class="source-button reference-add" type="button" data-pick>
    <span class="source-thumb">${I.add}</span><span><strong>${copy.addReferences}</strong>
      <small>${copy.referenceCount(references.length)} · ${references.length}/${MAX_REFERENCES}</small></span>
  </button>` : "";
  return list + add;
}

function renderDialog(): string {
  if (!dialog) return "";
  const rename = dialog.kind === "rename";
  return `<div class="app-dialog" role="dialog" aria-modal="true"><div class="dialog-surface">
    <strong>${rename ? copy.renameTitle : copy.deleteConfirm}</strong>
    ${rename ? `<input class="dialog-input" value="${escapeHtml(dialog.value)}"
      maxlength="180" aria-label="${copy.renameTitle}">` : ""}
    <div class="dialog-actions"><button class="secondary" type="button"
      data-dialog-dismiss>${copy.dismiss}</button><button class="${rename ? "primary" : "danger-action"}"
      type="button" data-dialog-accept>${rename ? copy.save : copy.delete}</button></div>
  </div></div>`;
}

function render() {
  const version = ++renderVersion;
  fitObserver?.disconnect();
  const selection = selected();
  const draft = currentDraft();
  const selectedJob = jobs.find((item) => item.jobId === selectedKey);
  const selectedBusy = Boolean(selectedJob && busy(selectedJob.stage));
  const ready = preparationStatus === "ready" || preparationStatus === "partial";
  const canCreate = Boolean(selection?.prompt.trim()) && !selectedBusy;
  const dimensions = selection?.width && selection?.height
    ? `${selection.width} × ${selection.height} px`
    : selection?.title ?? "";
  const stagePaths = [...(selection?.referencePaths ?? []), selection?.output].filter(
    (path): path is string => Boolean(path),
  );
  previews.retainStagePaths(stagePaths);

  app.innerHTML = `<section class="shell">
    <div class="drop-overlay">${I.image}<strong>${copy.dropImages}</strong></div>
    <header class="titlebar" data-drag><div class="identity"><span class="app-icon">${I.image}</span>
      <strong>${copy.title}</strong><span class="readiness ${ready ? "" : "busy"}">
      <i></i><span>${ready ? copy.ready : copy.preparing}</span></span></div>
      <div class="window-actions"><button class="icon-button" type="button" data-minimize
        title="${copy.minimize}">${I.minimize}</button><button class="icon-button close" type="button"
        data-close title="${copy.close}">${I.close}</button></div></header>
    <main class="workspace">
      <aside class="queue-rail"><div class="rail-heading"><span>${copy.queue}</span>
        <button class="icon-button add" type="button" data-new-session
          title="${copy.newSession}">${I.add}</button></div>
        <div class="queue-list">${renderQueue() || `<p class="queue-empty">${copy.emptyQueue}</p>`}</div></aside>
      <section class="stage"><div class="artboard-wrap"><div class="artboard">
        ${stageMarkup(selection, copy, compare)}</div>
        ${selectedJob ? `<div class="status-strip"><span class="status-icon">${I.sparkle}</span>
          <span class="status-copy"><span class="status-heading"><strong>${escapeHtml(statusLabel(selectedJob))}</strong></span>
          <small>${escapeHtml(selectedJob.stage === "failed" ? copy.failed : selectedJob.prompt)}</small></span>
          <i class="progress ${busy(selectedJob.stage) ? "visible" : ""}">
            <b style="width:${Math.round((selectedJob.progressRatio ?? 0) * 100)}%"></b></i></div>` : ""}
        ${selection?.output ? `<button class="floating-action" type="button"
          data-open="${escapeHtml(selection.output)}" title="${copy.openFolder}">${I.folder}</button>` : ""}
        </div><div class="result-meta">${escapeHtml(dimensions)}</div></section>
      <aside class="controls">
        <section><span class="label">${copy.image}</span>${renderReferences(selection, Boolean(draft))}</section>
        <section><label class="label" for="imagePrompt">${copy.instruction}</label>
          <div class="prompt-field"><textarea id="imagePrompt" maxlength="4000"
            placeholder="${copy.instructionHint}" ${draft ? "" : "disabled"}>${escapeHtml(selection?.prompt ?? "")}</textarea>
            <small><span data-prompt-count>${selection?.prompt.length ?? 0}</span> / 4000</small></div></section>
        <section><span class="label">${copy.saveTo}</span><button class="folder-row" type="button" data-output>
          ${I.folder}<span title="${escapeHtml(outputDir)}">${escapeHtml(outputDir || copy.change)}</span></button></section>
        <div class="action-area"><button class="primary" type="button" data-create ${canCreate ? "" : "disabled"}>
          ${I.sparkle}<span>${draft ? copy.generate : copy.generateAgain}</span></button>
          ${selectedJob && busy(selectedJob.stage) ? `<button class="secondary" type="button"
            data-cancel="${escapeHtml(selectedJob.jobId)}">${copy.cancel}</button>` : ""}</div>
      </aside>
    </main>${renderDialog()}<div class="app-toast ${message ? "visible" : ""}"
      role="status">${escapeHtml(message)}</div></section>`;

  bindEvents();
  hydrationQueue = hydrationQueue.then(() => hydrateImages(version)).catch(() => undefined);
}

async function hydrateImages(version: number) {
  if (version !== renderVersion) return;
  for (const image of document.querySelectorAll<HTMLImageElement>("[data-stage-path]")) {
    const path = image.dataset.stagePath;
    if (!path) continue;
    try {
      const source = await previews.stage(path, Number(image.dataset.stageEdge) || 1_600);
      if (version !== renderVersion || !image.isConnected) return;
      if (image.hasAttribute("data-fit-anchor")) {
        image.addEventListener("load", () => fitNaturalFrame(image), { once: true });
      }
      image.src = source;
      image.hidden = false;
    } catch { /* A missing preview does not change the job or session. */ }
  }
  for (const element of document.querySelectorAll<HTMLElement>("[data-thumb]")) {
    const path = element.dataset.thumb;
    if (!path) continue;
    try {
      const source = await previews.thumbnail(path);
      if (version !== renderVersion || !element.isConnected) return;
      element.style.backgroundImage = `url("${source}")`;
      element.innerHTML = "";
    } catch { /* Queue thumbnails are optional. */ }
  }
}

function fitNaturalFrame(image: HTMLImageElement) {
  const frame = image.closest<HTMLElement>("[data-fit-frame]");
  const artboard = image.closest<HTMLElement>(".artboard");
  if (!frame || !artboard || !image.naturalWidth || !image.naturalHeight) return;
  const ratio = image.naturalWidth / image.naturalHeight;
  const fit = () => {
    const width = Math.min(artboard.clientWidth * 0.88, artboard.clientHeight * 0.84 * ratio);
    frame.style.width = `${Math.max(1, width)}px`;
    frame.style.height = `${Math.max(1, width / ratio)}px`;
  };
  fitObserver?.disconnect();
  fitObserver = new ResizeObserver(fit);
  fitObserver.observe(artboard);
  fit();
}

function bindEvents() {
  document.querySelector("[data-new-session]")?.addEventListener("click", () => newSession());
  document.querySelectorAll("[data-pick]").forEach((element) =>
    element.addEventListener("click", () => void pickImages()));
  document.querySelector("[data-output]")?.addEventListener("click", () => void chooseOutput());
  document.querySelector("[data-create]")?.addEventListener("click", () => void submit());
  document.querySelector("[data-close]")?.addEventListener("click", () => void invoke("close_window"));
  document.querySelector("[data-minimize]")?.addEventListener("click", () => void invoke("minimize_window"));
  document.querySelector("[data-drag]")?.addEventListener("mousedown", (event) => {
    if (!(event.target as HTMLElement).closest("button")) void invoke("start_drag");
  });
  document.querySelector<HTMLTextAreaElement>("#imagePrompt")?.addEventListener("input", (event) => {
    const draft = currentDraft();
    if (!draft) return;
    draft.prompt = (event.target as HTMLTextAreaElement).value;
    const counter = document.querySelector("[data-prompt-count]");
    if (counter) counter.textContent = String(draft.prompt.length);
    const create = document.querySelector<HTMLButtonElement>("[data-create]");
    if (create) create.disabled = !draft.prompt.trim();
  });
  document.querySelector(".compare-input")?.addEventListener("input", (event) => {
    compare = Number((event.target as HTMLInputElement).value);
    document.querySelector<HTMLElement>(".comparison-frame")?.style.setProperty("--compare", `${compare}%`);
  });
  document.querySelectorAll<HTMLElement>("[data-select]").forEach((element) =>
    element.addEventListener("click", () => {
      selectedKey = element.dataset.select || "";
      message = "";
      render();
    }));
  document.querySelectorAll<HTMLElement>("[data-remove-reference]").forEach((element) =>
    element.addEventListener("click", () => {
      const draft = currentDraft();
      const index = Number(element.dataset.removeReference);
      if (draft && Number.isInteger(index)) draft.referencePaths.splice(index, 1);
      render();
    }));
  document.querySelectorAll<HTMLElement>("[data-remove-session]").forEach((element) =>
    element.addEventListener("click", () => {
      const key = element.dataset.removeSession;
      drafts = drafts.filter((item) => item.key !== key);
      if (selectedKey === key) {
        selectedKey = drafts[0]?.key ?? jobs[jobs.length - 1]?.jobId ?? history[0]?.id ?? "";
      }
      render();
    }));
  document.querySelectorAll<HTMLElement>("[data-cancel]").forEach((element) =>
    element.addEventListener("click", () => void cancel(element.dataset.cancel || "")));
  document.querySelectorAll<HTMLElement>("[data-open]").forEach((element) =>
    element.addEventListener("click", () => void invoke("open_output", { path: element.dataset.open })));
  bindHistoryEvents();
}

function bindHistoryEvents() {
  document.querySelectorAll<HTMLElement>("[data-rename]").forEach((element) => {
    const entry = history.find((item) => item.id === element.dataset.rename);
    if (entry) element.addEventListener("click", () => {
      dialog = { kind: "rename", entry, value: entry.outputName };
      render();
      document.querySelector<HTMLInputElement>(".dialog-input")?.select();
    });
  });
  document.querySelectorAll<HTMLElement>("[data-delete]").forEach((element) => {
    const entry = history.find((item) => item.id === element.dataset.delete);
    if (entry) element.addEventListener("click", () => {
      dialog = { kind: "delete", entry, value: "" };
      render();
    });
  });
  document.querySelector(".dialog-input")?.addEventListener("input", (event) => {
    if (dialog) dialog.value = (event.target as HTMLInputElement).value;
  });
  document.querySelector("[data-dialog-dismiss]")?.addEventListener("click", () => {
    dialog = null;
    render();
  });
  document.querySelector("[data-dialog-accept]")?.addEventListener("click", () => void commitDialog());
}

window.handleNativeFileDrop = (paths) => {
  document.body.classList.remove("file-dragging");
  attachReferences(paths);
};
window.handleNativeFileDrag = (active) => document.body.classList.toggle("file-dragging", active);
window.applyHostContext = (next) => {
  if (next.theme) document.documentElement.dataset.theme = next.theme;
  if (next.language) {
    document.documentElement.lang = next.language;
    copy = copyFor(next.language);
  }
  render();
};

async function bootstrap() {
  document.documentElement.dataset.theme = context.theme || "dark";
  document.documentElement.lang = context.language || "en";
  outputDir = await invoke<string>("default_output_dir");
  await invoke("prepare_runtime");
  await Promise.all([refresh(), updateReady()]);
  setInterval(() => void refresh(), 1_000);
  setInterval(() => void updateReady(), 2_500);
}

newSession(false);
render();
void bootstrap();
