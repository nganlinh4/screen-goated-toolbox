import { MATERIAL_SYMBOLS } from "../../ui-shared/material-symbols";

export const ICONS = {
  model: MATERIAL_SYMBOLS.deployedCode,
  image: MATERIAL_SYMBOLS.image,
  folder: MATERIAL_SYMBOLS.folder,
  sparkle: MATERIAL_SYMBOLS.autoAwesome,
  stop: MATERIAL_SYMBOLS.stop,
  close: MATERIAL_SYMBOLS.close,
  minimize: MATERIAL_SYMBOLS.remove,
  check: MATERIAL_SYMBOLS.check,
  add: MATERIAL_SYMBOLS.add,
  palette: MATERIAL_SYMBOLS.palette,
  toon: MATERIAL_SYMBOLS.texture,
  outline: MATERIAL_SYMBOLS.outline,
  rotate: MATERIAL_SYMBOLS.rotate360,
  grid: MATERIAL_SYMBOLS.gridOn,
  wire: MATERIAL_SYMBOLS.viewInAr,
  fit: MATERIAL_SYMBOLS.fitScreen,
  rename: MATERIAL_SYMBOLS.edit,
  trash: MATERIAL_SYMBOLS.delete,
};

export function appMarkup() {
  return `
  <section class="app-shell">
    <div class="drop-overlay">${ICONS.image}<strong data-i18n="dropImages"></strong></div>
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
          <span class="rail-actions"><button class="icon-button" id="deleteAllHistory" type="button" data-i18n-title="deleteAll">${ICONS.trash}</button><button class="icon-button add-button" id="addImagesButton" type="button" data-i18n-title="addImages">${ICONS.add}</button></span>
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
        <aside class="reference-preview" id="referencePreview" data-i18n-aria="referencePreview" hidden>
          <header class="reference-preview-header">
            <span>${ICONS.image}</span>
            <strong id="referencePreviewName"></strong>
            <button class="view-tool" id="referencePreviewClose" type="button" data-i18n-title="close">${ICONS.close}</button>
          </header>
          <div class="reference-preview-image"><img id="referencePreviewImage" alt="" /></div>
        </aside>
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
          <span class="control-label" data-i18n="generationMode"></span>
          <div class="mode-options" role="group" data-i18n-aria="generationMode">
            <button type="button" data-generation-mode="fast" data-i18n="fast"></button>
            <button type="button" data-generation-mode="quality" data-i18n="quality"></button>
          </div>
        </div>
        <div class="control-section instruction-section" id="instructionSection" hidden>
          <label class="control-label" for="instructionInput" data-i18n="optionalInstruction"></label>
          <textarea id="instructionInput" maxlength="1000" data-i18n-placeholder="optionalInstructionHint"></textarea>
        </div>
        <div class="control-section">
          <div class="control-heading">
            <label for="polycountRange" data-i18n="topology"></label>
            <output id="polycountValue">5,000</output>
          </div>
          <input class="range" id="polycountRange" type="range" min="100" max="20000" step="100" value="5000" />
          <div class="range-scale"><span data-i18n="light"></span><span data-i18n="detailed"></span></div>
        </div>
        <div class="control-section compact">
          <button class="folder-row" id="chooseFolderButton" type="button">
            <span>${ICONS.folder}</span><span><small data-i18n="saveTo"></small><strong id="folderName"></strong></span>
          </button>
        </div>
        <div class="control-section compact" id="autoSegmentSection">
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
        <button class="cancel-action" id="cancelButton" type="button"><span>${ICONS.stop}</span><span id="cancelLabel" data-i18n="cancel"></span></button>
      </aside>
    </main>
    <div class="app-toast" id="appToast" role="status" aria-live="polite"></div>
  </section>`;
}
