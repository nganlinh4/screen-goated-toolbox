import type { Copy } from "./i18n";
import { ICONS as I } from "./icons";
import { escapeHtml, type Selection } from "./models";

function preview(path: string, cssClass: string, maxEdge: number, alt: string, fit = false) {
  return `<img class="${cssClass}" data-stage-path="${escapeHtml(path)}"
    data-stage-edge="${maxEdge}" ${fit ? "data-fit-anchor" : ""}
    data-preview-pending="true" alt="${escapeHtml(alt)}">`;
}

function referenceGrid(selection: Selection, copy: Copy) {
  return `<div class="reference-grid">
    ${selection.referencePaths.map((path, index) => `
      <figure class="reference-cell">
        ${preview(path, "reference-image", 640, `${copy.reference} ${index + 1}`)}
        <figcaption>${index + 1}</figcaption>
      </figure>`).join("")}
  </div>`;
}

export function stageMarkup(selection: Selection | null, copy: Copy, compare: number): string {
  if (!selection) {
    return `<div class="empty-state">${I.image}<strong>${copy.newSession}</strong>
      <span>${copy.newSessionHint}</span></div>`;
  }

  const references = selection.referencePaths;
  const output = selection.output;
  if (!output && references.length === 0) {
    return `<div class="empty-state">${I.sparkle}<strong>${copy.textOnlyTitle}</strong>
      <span>${copy.textOnlyHint}</span></div>`;
  }
  if (!output && references.length === 1) {
    return `<div class="natural-frame" data-fit-frame>
      ${preview(references[0], "natural-image", 1_600, copy.source, true)}
      <span class="compare-label before-label">${copy.source}</span>
    </div>`;
  }
  if (!output) return referenceGrid(selection, copy);

  if (references.length === 0) {
    return `<div class="natural-frame" data-fit-frame>
      ${preview(output, "natural-image", 1_600, copy.after, true)}
    </div>`;
  }
  if (references.length === 1) {
    return `<div class="comparison-frame has-output" data-fit-frame style="--compare:${compare}%">
      ${preview(references[0], "comparison-image before-image", 1_600, copy.before, true)}
      <div class="after-layer">
        ${preview(output, "comparison-image after-image", 1_600, copy.after)}
      </div>
      <span class="compare-label before-label">${copy.before}</span>
      <span class="compare-label after-label">${copy.after}</span>
      <span class="compare-seam"><i>${I.swap}</i></span>
      <input class="compare-input" type="range" min="0" max="100"
        value="${compare}" aria-label="${copy.comparison}">
    </div>`;
  }
  return `<div class="multi-result-layout">
    <div class="multi-output">${preview(output, "multi-output-image", 1_600, copy.after)}</div>
    <div class="reference-strip" aria-label="${copy.references}">
      ${references.map((path, index) => `
        <span>${preview(path, "reference-image", 512, `${copy.reference} ${index + 1}`)}</span>
      `).join("")}
    </div>
  </div>`;
}
