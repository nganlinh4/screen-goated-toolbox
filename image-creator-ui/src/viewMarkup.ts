import type { Copy } from "./i18n";
import { ICONS as I } from "./icons";
import {
  escapeHtml,
  historyReferences,
  jobReferences,
  MAX_REFERENCES,
  pathName,
  type DialogState,
  type DraftSession,
  type HistoryEntry,
  type JobStatus,
  type Selection,
} from "./models";
import { imageStatusLabel, publicImageStage } from "./jobPresentation";
import { newestSessionsFirst } from "../../ui-shared/creation-history-order";

export function referenceTitle(paths: string[], copy: Copy): string {
  if (paths.length === 0) return copy.newImage;
  if (paths.length === 1) return pathName(paths[0]);
  return copy.referenceCount(paths.length);
}

export function renderImageQueue(
  jobs: JobStatus[],
  drafts: DraftSession[],
  history: HistoryEntry[],
  selectedKey: string,
  copy: Copy,
): string {
  const knownOutputs = new Set(
    jobs.map((job) => job.outputPath?.toLocaleLowerCase()).filter((path): path is string => Boolean(path)),
  );
  const jobRows = jobs.map((job) => {
    const references = jobReferences(job);
    return { createdAtMs: job.createdAtMs, markup: `<div class="queue-item ${selectedKey === job.jobId ? "selected" : ""}"
      data-job-row="${escapeHtml(job.jobId)}">
      <button class="queue-item-main" type="button" data-select="${escapeHtml(job.jobId)}">
        <span class="queue-thumb">${I.image}</span>
        <span class="queue-copy"><strong>${escapeHtml(job.outputName || referenceTitle(references, copy))}</strong>
          <small data-job-status>${escapeHtml(imageStatusLabel(job, copy))}</small></span>
        <i class="queue-state state ${publicImageStage(job.stage)}" data-job-state></i>
      </button>
    </div>` };
  });
  const draftRows = drafts.map((draft) => ({ createdAtMs: draft.createdAtMs, markup: `
    <div class="queue-item ${selectedKey === draft.key ? "selected" : ""}">
      <button class="queue-item-main" type="button" data-select="${escapeHtml(draft.key)}">
        <span class="queue-thumb">${draft.referencePaths.length ? I.image : I.sparkle}</span>
        <span class="queue-copy"><strong>${escapeHtml(referenceTitle(draft.referencePaths, copy))}</strong>
          <small>${draft.referencePaths.length ? copy.referenceReady : copy.noReferences}</small></span>
        <i class="queue-state state done"></i>
      </button>
      <span class="queue-actions"><button type="button" class="danger"
        data-remove-session="${escapeHtml(draft.key)}" title="${copy.delete}">${I.trash}</button></span>
    </div>` }));
  const historyRows = history
    .filter((entry) => !knownOutputs.has(entry.outputPath.toLocaleLowerCase()))
    .map((entry) => ({ createdAtMs: entry.createdAtMs, markup: `
      <div class="queue-item ${selectedKey === entry.id ? "selected" : ""}">
        <button class="queue-item-main" type="button" data-select="${escapeHtml(entry.id)}">
          <span class="queue-thumb">${I.image}</span>
          <span class="queue-copy"><strong>${escapeHtml(entry.outputName)}</strong>
            <small>${copy.savedResult}</small></span><i class="queue-state state done"></i>
        </button>
        <span class="queue-actions">
          <button type="button" data-rename="${escapeHtml(entry.id)}" title="${copy.rename}">${I.rename}</button>
          <button type="button" class="danger" data-delete="${escapeHtml(entry.id)}"
            title="${copy.delete}">${I.trash}</button>
        </span>
      </div>` }));
  return newestSessionsFirst([...jobRows, ...draftRows, ...historyRows])
    .map((row) => row.markup)
    .join("");
}

export function renderImageReferences(
  selection: Selection | null,
  editable: boolean,
  copy: Copy,
): string {
  const references = selection?.referencePaths ?? [];
  const list = references.length ? `<div class="reference-list">
    ${references.map((path, index) => `<div class="reference-chip">
      <span>${I.image}</span>
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

export function renderImageDialog(dialog: DialogState | null, copy: Copy): string {
  if (!dialog) return "";
  return `<div class="app-dialog" role="dialog" aria-modal="true"><div class="dialog-surface">
    <strong>${copy.renameTitle}</strong>
    <input class="dialog-input" value="${escapeHtml(dialog.value)}"
      maxlength="180" aria-label="${copy.renameTitle}">
    <div class="dialog-actions"><button class="secondary" type="button"
      data-dialog-dismiss>${copy.dismiss}</button><button class="primary"
      type="button" data-dialog-accept>${copy.save}</button></div>
  </div></div>`;
}
