import "./styles.css";
import "../../ui-shared/creation-shell-layout.css";
import { DemandPoller } from "../../ui-shared/demand-poller";
import { copyFor, type Copy } from "./i18n";
import { ICONS as I } from "./icons";
import { confirmDeleteAll, deleteSavedResults, renameSavedResult } from "./history-actions";
import {
  ImageProgressPresenter,
  imageStatusLabel,
  publicImageStage,
} from "./jobPresentation";
import {
  escapeHtml,
  historyReferences,
  jobReferences,
  MAX_REFERENCES,
  orderedPaths,
  type DialogState,
  type DraftSession,
  type HistoryEntry,
  type JobStatus,
  type Selection,
} from "./models";
import { guardPollRendering } from "./pollRenderGuard";
import { ImagePreviewHydrator } from "./previewHydration";
import { PreviewStore } from "./previewStore";
import { stageMarkup } from "./stageMarkup";
import {
  canSubmitImageSelection,
  ExplicitSubmissionTracker,
  selectionAfterSubmission,
  startImageArguments,
  SurfaceSourceRegistry,
} from "./submission";
import {
  referenceTitle,
  renderImageDialog,
  renderImageQueue,
  renderImageReferences,
} from "./viewMarkup";
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
document.documentElement.dataset.creationShellOnly = "false";

let copy: Copy = copyFor(context.language);
let drafts: DraftSession[] = [];
let jobs: JobStatus[] = [];
let history: HistoryEntry[] = [];
let selectedKey = "";
let outputDir = "";
let compare = 50;
let message = "";
let preparationStatus = "ready";
let dialog: DialogState | null = null;
let renderedLayoutSignature = "";
let syncedPollSignature = "";
let sessionSequence = 0;
let fitObserver: ResizeObserver | undefined;
const submissions = new ExplicitSubmissionTracker();
const surfaceSources = new SurfaceSourceRegistry();

async function invoke<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  if (!window.invoke) throw new Error("Native host is unavailable");
  return window.invoke<T>(command, args);
}

const previews = new PreviewStore(invoke);
const previewHydrator = new ImagePreviewHydrator(previews, fitNaturalFrame);
const progressPresenter = new ImageProgressPresenter();
const renderAfterPoll = guardPollRendering(app, render);
const jobMonitor = new DemandPoller({
  hasWork: () => jobs.some((job) => busy(job.stage)),
  poll: async () => {
    if (await refreshJobs()) await refreshHistory();
  },
  present: () => {
    const job = jobs.find((item) => item.jobId === selectedKey);
    if (job && busy(job.stage)) progressPresenter.sync(app, job, copy);
  },
  pollEveryMs: 900,
  presentEveryMs: 250,
});
app.addEventListener("pointerdown", () => previewHydrator.setInteractionActive(true), true);
window.addEventListener("pointerup", () => previewHydrator.setInteractionActive(false), true);
window.addEventListener("pointercancel", () => previewHydrator.setInteractionActive(false), true);
window.addEventListener("pagehide", () => jobMonitor.dispose(), { once: true });

function terminal(stage: string): boolean {
  return stage === "done" || stage === "failed" || stage === "cancelled";
}

function busy(stage: string): boolean {
  return !terminal(stage) && stage !== "draft";
}

function currentDraft(): DraftSession | undefined {
  return drafts.find((item) => item.key === selectedKey);
}

function selected(): Selection | null {
  const job = jobs.find((item) => item.jobId === selectedKey);
  if (job) {
    const importedReferences = surfaceSources.references(job.jobId);
    const references = importedReferences ?? jobReferences(job);
    return {
      key: job.jobId,
      kind: "job",
      referencePaths: references,
      sourceProvenance: importedReferences
        ? references.length ? "surface-import" : "none"
        : references.length ? "presentation" : "none",
      output: job.outputPath,
      title: job.outputName || referenceTitle(references, copy),
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
      sourceProvenance: historyReferences(entry).length ? "presentation" : "none",
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
    sourceProvenance: draft.referencePaths.length ? "surface-import" : "none",
    title: referenceTitle(draft.referencePaths, copy),
    prompt: draft.prompt,
  } : null;
}

function newSession(renderNow = true) {
  const draft: DraftSession = {
    key: `draft-${Date.now()}-${++sessionSequence}`,
    referencePaths: [],
    prompt: "",
    createdAtMs: Date.now(),
  };
  drafts = [...drafts, draft];
  selectedKey = draft.key;
  message = "";
  if (renderNow) render();
}

function attachReferences(paths: string[]) {
  if (paths.length > MAX_REFERENCES) {
    message = copy.referenceLimit(MAX_REFERENCES);
    render();
    return;
  }
  let draft = currentDraft();
  if (!draft) {
    newSession(false);
    draft = currentDraft();
  }
  if (!draft) return;
  const combined = orderedPaths([...draft.referencePaths, ...paths]);
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
  const frozenPrompt = selection.prompt.trim();
  if (!frozenPrompt) {
    message = copy.promptRequired;
    render();
    return;
  }
  if (!canSubmitImageSelection(selection.referencePaths, selection.sourceProvenance)) {
    message = copy.reselectReferences;
    render();
    return;
  }

  const ticket = submissions.begin(selection.key);
  const draftKey = selection.kind === "draft" ? selection.key : undefined;
  render();
  try {
    const status = await invoke<JobStatus>(
      "start_job",
      startImageArguments(
        selection.referencePaths,
        outputDir,
        frozenPrompt,
        selection.sourceProvenance,
      ),
    );
    status.createdAtMs = Date.now();
    surfaceSources.remember(status.jobId, selection.referencePaths);
    jobs = [...jobs.filter((item) => item.jobId !== status.jobId), status];
    const latest = submissions.isLatest(ticket);
    const ownsPresentation = latest && selectedKey === ticket.sourceKey;
    if (draftKey && latest) drafts = drafts.filter((item) => item.key !== draftKey);
    selectedKey = selectionAfterSubmission(selectedKey, ticket, status.jobId, latest);
    if (ownsPresentation) message = "";
    if (terminal(status.stage)) await refreshHistory();
    else jobMonitor.start();
  } catch {
    if (submissions.isLatest(ticket) && selectedKey === ticket.sourceKey) {
      message = copy.failed;
    }
  } finally {
    submissions.finish(ticket);
  }
  render();
}

function reconcileSelection() {
  const exists = jobs.some((item) => item.jobId === selectedKey)
    || drafts.some((item) => item.key === selectedKey)
    || history.some((item) => item.id === selectedKey);
  if (!exists) selectedKey = jobs[jobs.length - 1]?.jobId ?? drafts[0]?.key ?? history[0]?.id ?? "";
  reconcileAfterPoll();
}

async function refreshJobs(): Promise<boolean> {
  try {
    const previous = new Map(jobs.map((job) => [job.jobId, job.stage]));
    const creationTimes = new Map(jobs.map((job) => [job.jobId, job.createdAtMs]));
    const refreshed = await invoke<JobStatus[]>("job_statuses");
    refreshed.forEach((job) => {
      job.createdAtMs = terminal(job.stage) && !terminal(previous.get(job.jobId) || "")
        ? Date.now()
        : creationTimes.get(job.jobId) ?? Date.now() - Math.max(0, job.elapsedMs || 0);
    });
    const reachedTerminal = refreshed.some((job) =>
      terminal(job.stage) && previous.get(job.jobId) !== job.stage);
    jobs = refreshed;
    progressPresenter.retain(jobs);
    reconcileSelection();
    return reachedTerminal;
  } catch {
    return false;
  }
}

async function refreshHistory() {
  try {
    history = await invoke<HistoryEntry[]>("history_results");
    reconcileSelection();
  } catch {
    // A hidden or closing host can reject a poll; the next poll reconciles it.
  }
}

async function cancel(jobId: string) {
  jobs = await invoke<JobStatus[]>("cancel_job", { jobId });
  if (jobs.some((job) => busy(job.stage))) jobMonitor.start();
  else await refreshHistory();
  render();
}

async function commitDialog() {
  if (!dialog) return;
  const current = dialog;
  dialog = null;
  if (!await renameSavedResult(invoke, current.entry, current.value)) {
    message = copy.rename;
    render();
    return;
  }
  await Promise.all([refreshJobs(), refreshHistory()]);
}

async function deleteHistory(entry?: HistoryEntry) {
  if (!entry && !await confirmDeleteAll(copy)) return;
  if (!await deleteSavedResults(invoke, entry?.id)) {
    message = copy.delete;
    render();
    return;
  }
  if (entry?.id === selectedKey || (!entry && history.some((item) => item.id === selectedKey))) {
    selectedKey = "";
  }
  await Promise.all([refreshJobs(), refreshHistory()]);
  render();
}
function layoutSignature(): string {
  return JSON.stringify({
    selectedKey,
    outputDir,
    message,
    submitting: submissions.activeIds(),
    preparationReady: preparationStatus === "ready",
    dialog,
    drafts: drafts.map((draft) => [draft.key, draft.prompt, draft.referencePaths]),
    jobs: jobs.map((job) => [
      job.jobId,
      terminal(job.stage),
      job.outputPath || "",
      job.outputName || "",
      job.prompt,
      job.width || 0,
      job.height || 0,
      jobReferences(job),
    ]),
    history: history.map((entry) => [
      entry.id,
      entry.outputPath,
      entry.outputName,
      entry.metadata?.prompt || "",
      historyReferences(entry),
    ]),
  });
}

function syncPolledUi() {
  const ready = preparationStatus === "ready";
  const readiness = document.querySelector<HTMLElement>(".readiness");
  readiness?.classList.toggle("busy", !ready);
  const readinessText = readiness?.querySelector<HTMLElement>("span");
  if (readinessText) readinessText.textContent = ready ? copy.ready : copy.preparing;

  const selectedJob = jobs.find((item) => item.jobId === selectedKey);
  for (const job of jobs) {
    const row = document.querySelector<HTMLElement>(`[data-job-row="${CSS.escape(job.jobId)}"]`);
    const label = row?.querySelector<HTMLElement>("[data-job-status]");
    const state = row?.querySelector<HTMLElement>("[data-job-state]");
    if (label) label.textContent = imageStatusLabel(job, copy);
    if (state) {
      state.classList.remove(
        "queued",
        "preparing",
        "uploading",
        "generating",
        "finalizing",
        "done",
        "failed",
        "cancelled",
      );
      state.classList.add(publicImageStage(job.stage));
    }
  }
  const selectedStatus = document.querySelector<HTMLElement>("[data-selected-job-status]");
  if (selectedStatus && selectedJob) {
    selectedStatus.textContent = imageStatusLabel(selectedJob, copy);
  }
  progressPresenter.sync(app, selectedJob, copy);
}

function pollUiSignature() {
  return JSON.stringify({
    selectedKey,
    ready: preparationStatus === "ready",
    jobs: jobs.map((job) => [job.jobId, publicImageStage(job.stage)]),
  });
}

function reconcileAfterPoll() {
  if (layoutSignature() !== renderedLayoutSignature) renderAfterPoll();
  const signature = pollUiSignature();
  if (signature !== syncedPollSignature) {
    syncPolledUi();
    syncedPollSignature = signature;
  }
}

function render(preserveQueue = false) {
  const retainedQueue = preserveQueue
    ? app.querySelector<HTMLElement>(".queue-rail")
    : null;
  fitObserver?.disconnect();
  const selection = selected();
  const draft = currentDraft();
  const selectedJob = jobs.find((item) => item.jobId === selectedKey);
  const progressState = progressPresenter.snapshot(selectedJob, copy);
  const ready = preparationStatus === "ready";
  const canUseSource = Boolean(
    selection
      && canSubmitImageSelection(selection.referencePaths, selection.sourceProvenance),
  );
  const canCreate = Boolean(selection?.prompt.trim()) && canUseSource;
  const dimensions = selection?.width && selection?.height
    ? `${selection.width} × ${selection.height} px`
    : selection?.title ?? "";
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
        <span class="rail-actions"><button class="icon-button" type="button" data-delete-all
          title="${copy.deleteAll}">${I.trash}</button><button class="icon-button add" type="button"
          data-new-session title="${copy.newSession}">${I.add}</button></span></div>
        <div class="queue-list">${renderImageQueue(jobs, drafts, history, selectedKey, copy)
          || `<p class="queue-empty">${copy.emptyQueue}</p>`}</div></aside>
      <section class="stage"><div class="artboard-wrap"><div class="artboard">
        ${stageMarkup(selection, copy, compare)}</div>
        ${selectedJob ? `<div class="status-strip"><span class="status-icon">${I.sparkle}</span>
          <span class="status-copy"><span class="status-heading"><strong data-selected-job-status>${escapeHtml(imageStatusLabel(selectedJob, copy))}</strong>
          <small class="status-eta ${progressState.visible ? "visible" : ""}" data-job-progress-eta>${escapeHtml(progressState.eta)}</small></span>
          <small>${escapeHtml(selectedJob.stage === "failed" ? copy.failed : selectedJob.prompt)}</small></span>
          <i class="progress ${busy(selectedJob.stage) ? "visible" : ""}" data-job-progress
            role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-valuenow="${progressState.percent}">
            <b style="width:${progressState.percent}%"></b></i></div>` : ""}
        ${selection?.output ? `<button class="floating-action" type="button"
          data-open="${escapeHtml(selection.output)}" title="${copy.openFolder}">${I.folder}</button>` : ""}
        </div><div class="result-meta">${escapeHtml(dimensions)}</div></section>
      <aside class="controls">
        <section><span class="label">${copy.image}</span>${renderImageReferences(selection, Boolean(draft), copy)}</section>
        <section><label class="label" for="imagePrompt">${copy.instruction}</label>
          <div class="prompt-field"><textarea id="imagePrompt" maxlength="4000"
            placeholder="${copy.instructionHint}" ${draft ? "" : "disabled"}>${escapeHtml(selection?.prompt ?? "")}</textarea>
            <small><span data-prompt-count>${selection?.prompt.length ?? 0}</span> / 4000</small></div></section>
        <section><span class="label">${copy.saveTo}</span><button class="folder-row" type="button" data-output>
          ${I.folder}<span title="${escapeHtml(outputDir)}">${escapeHtml(outputDir || copy.change)}</span></button></section>
        <div class="action-area"><button class="primary" type="button" data-create
          ${canCreate ? "" : "disabled"} title="${canUseSource ? "" : copy.reselectReferences}">
          ${I.sparkle}<span>${draft ? copy.generate : copy.generateAgain}</span></button>
          ${selectedJob && busy(selectedJob.stage) ? `<button class="secondary" type="button"
            data-cancel="${escapeHtml(selectedJob.jobId)}">${copy.cancel}</button>` : ""}</div>
      </aside>
    </main>${renderImageDialog(dialog, copy)}<div class="app-toast ${message ? "visible" : ""}"
      role="status">${escapeHtml(message)}</div></section>`;

  if (retainedQueue) {
    app.querySelector(".queue-rail")?.replaceWith(retainedQueue);
    retainedQueue.querySelectorAll<HTMLElement>("[data-select]").forEach((element) => {
      const selected = element.dataset.select === selectedKey;
      element.closest(".queue-item")?.classList.toggle("selected", selected);
      element.setAttribute("aria-current", selected ? "true" : "false");
    });
  }
  bindEvents(!retainedQueue);
  renderedLayoutSignature = layoutSignature();
  syncedPollSignature = pollUiSignature();
  previewHydrator.bind(app);
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

function bindEvents(bindQueue = true) {
  if (bindQueue) {
    document.querySelector("[data-delete-all]")?.addEventListener(
      "click",
      () => void deleteHistory(),
    );
    document.querySelector("[data-new-session]")?.addEventListener("click", () => newSession());
  }
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
    previewHydrator.hold(220);
    compare = Number((event.target as HTMLInputElement).value);
    document.querySelector<HTMLElement>(".comparison-frame")?.style.setProperty("--compare", `${compare}%`);
  });
  if (bindQueue) {
    document.querySelectorAll<HTMLElement>("[data-select]").forEach((element) =>
      element.addEventListener("click", () => {
        selectedKey = element.dataset.select || "";
        message = "";
        render(true);
      }));
  }
  document.querySelectorAll<HTMLElement>("[data-remove-reference]").forEach((element) =>
    element.addEventListener("click", () => {
      const draft = currentDraft();
      const index = Number(element.dataset.removeReference);
      if (draft && Number.isInteger(index)) draft.referencePaths.splice(index, 1);
      render();
    }));
  if (bindQueue) {
    document.querySelectorAll<HTMLElement>("[data-remove-session]").forEach((element) =>
      element.addEventListener("click", () => {
        const key = element.dataset.removeSession;
        drafts = drafts.filter((item) => item.key !== key);
        if (selectedKey === key) {
          selectedKey = drafts[0]?.key ?? jobs[jobs.length - 1]?.jobId ?? history[0]?.id ?? "";
        }
        render();
      }));
  }
  document.querySelectorAll<HTMLElement>("[data-cancel]").forEach((element) =>
    element.addEventListener("click", () => void cancel(element.dataset.cancel || "")));
  document.querySelectorAll<HTMLElement>("[data-open]").forEach((element) =>
    element.addEventListener("click", () => void invoke("open_output", { path: element.dataset.open })));
  if (bindQueue) bindHistoryEvents();
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
    if (entry) element.addEventListener("click", () => void deleteHistory(entry));
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

async function initializeApp() {
  document.documentElement.dataset.theme = context.theme || "dark";
  document.documentElement.lang = context.language || "en";
  outputDir = await invoke<string>("default_output_dir");
  await Promise.all([refreshJobs(), refreshHistory()]);
  jobMonitor.start();
  void invoke("prepare_runtime").catch(() => undefined);
}

newSession(false);
render();
void initializeApp();
window.addEventListener("focus", () => void refreshHistory());
