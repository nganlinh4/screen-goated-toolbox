import type { Copy } from "./i18n";
import { ICONS as I } from "./icons";
import { escapeHtml, type Selection } from "./models";
import { selectedImagePreviewPaths } from "./preview-policy";

function preview(path: string, cssClass: string, maxEdge: number, alt: string, fit = false) {
  return `<img class="${cssClass}" data-stage-path="${escapeHtml(path)}"
    data-stage-edge="${maxEdge}" ${fit ? "data-fit-anchor" : ""}
    data-preview-pending="true" decoding="async" alt="${escapeHtml(alt)}">`;
}

export function stageMarkup(selection: Selection | null, copy: Copy, compare: number): string {
  if (!selection) {
    return `<div class="empty-state">${I.image}<strong>${copy.newSession}</strong>
      <span>${copy.newSessionHint}</span></div>`;
  }

  const references = selection.referencePaths;
  const output = selection.output;
  const selectedPreviews = selectedImagePreviewPaths(references, output);
  if (!output && references.length === 0) {
    return `<div class="empty-state">${I.sparkle}<strong>${copy.textOnlyTitle}</strong>
      <span>${copy.textOnlyHint}</span></div>`;
  }
  if (!output) {
    return `<div class="natural-frame" data-fit-frame>
      ${preview(references[0], "natural-image", 1_600, copy.source, true)}
      <span class="compare-label before-label">${copy.source}</span>
      ${references.length > 1
        ? `<span class="compare-label after-label">${copy.referenceCount(references.length)}</span>`
        : ""}
    </div>`;
  }

  if (references.length === 0) {
    return `<div class="natural-frame" data-fit-frame>
      ${preview(selectedPreviews[0], "natural-image", 1_600, copy.after, true)}
    </div>`;
  }
  if (references.length === 1) {
    return `<div class="comparison-frame has-output" data-fit-frame style="--compare:${compare}%">
      ${preview(selectedPreviews[0], "comparison-image before-image", 1_600, copy.before, true)}
      <div class="after-layer">
        ${preview(selectedPreviews[1], "comparison-image after-image", 1_600, copy.after)}
      </div>
      <span class="compare-label before-label">${copy.before}</span>
      <span class="compare-label after-label">${copy.after}</span>
      <span class="compare-seam"><i>${I.swap}</i></span>
      <input class="compare-input" type="range" min="0" max="100"
        value="${compare}" aria-label="${copy.comparison}">
    </div>`;
  }
  return `<div class="natural-frame" data-fit-frame>
    ${preview(selectedPreviews[0], "natural-image", 1_600, copy.after, true)}
    <span class="compare-label before-label">${copy.after}</span>
    <span class="compare-label after-label">${copy.referenceCount(references.length)}</span>
    </div>
  `;
}
