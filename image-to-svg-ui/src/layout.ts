import { MATERIAL_SYMBOLS } from "../../ui-shared/material-symbols";
import { t } from "./i18n";

export const SVG_ICONS = {
  vector: MATERIAL_SYMBOLS.drawCollage,
  image: MATERIAL_SYMBOLS.image,
  add: MATERIAL_SYMBOLS.add,
  folder: MATERIAL_SYMBOLS.folder,
  sparkle: MATERIAL_SYMBOLS.autoAwesome,
  close: MATERIAL_SYMBOLS.close,
  minimize: MATERIAL_SYMBOLS.remove,
  zoomIn: MATERIAL_SYMBOLS.zoomIn,
  zoomOut: MATERIAL_SYMBOLS.zoomOut,
  fit: MATERIAL_SYMBOLS.fitScreen,
  checker: MATERIAL_SYMBOLS.gridOn,
  outline: MATERIAL_SYMBOLS.selectAll,
  undo: MATERIAL_SYMBOLS.undo,
  redo: MATERIAL_SYMBOLS.redo,
  trash: MATERIAL_SYMBOLS.delete,
  rename: MATERIAL_SYMBOLS.edit,
  save: MATERIAL_SYMBOLS.save,
  edit: MATERIAL_SYMBOLS.edit,
};

export function svgAppMarkup() {
  const icon = SVG_ICONS;
  return `
<section class="shell">
  <div class="drop-overlay" id="dropOverlay">${icon.image}<strong>${t("dropImages")}</strong></div>
  <header class="titlebar" id="dragRegion">
    <div class="identity"><span class="app-icon">${icon.vector}</span><strong>${t("title")}</strong><span class="readiness" id="readiness"><i></i><span id="readyText">${t("ready")}</span></span></div>
    <div class="window-actions"><button class="icon-button" id="minimize" title="${t("minimize")}">${icon.minimize}</button><button class="icon-button close" id="close" title="${t("close")}">${icon.close}</button></div>
  </header>
  <main class="workspace">
    <aside class="queue-rail">
      <div class="rail-heading"><span>${t("queue")}</span><span class="rail-actions"><button class="icon-button" id="deleteAllHistory" title="${t("deleteAll")}">${icon.trash}</button><button class="icon-button add" id="addImages" title="${t("addImages")}">${icon.add}</button></span></div>
      <div class="queue-list" id="queueList"></div>
    </aside>
    <section class="stage">
      <div class="artboard-wrap">
        <div class="artboard" id="artboard"><div class="empty-state">${icon.vector}<strong>${t("canvasEmpty")}</strong><span>${t("canvasHint")}</span></div></div>
        <div class="canvas-toolbar" id="viewerToolbar" hidden><button class="view-button" id="zoomOut" title="${t("zoomOut")}">${icon.zoomOut}</button><output id="zoomValue">100%</output><button class="view-button" id="zoomIn" title="${t("zoomIn")}">${icon.zoomIn}</button><button class="view-button" id="fitView" title="${t("fitView")}">${icon.fit}</button><i></i><button class="view-button" id="canvasBackground" title="${t("canvasBackground")}">${icon.checker}</button><button class="view-button" id="showOutlines" title="${t("showOutlines")}">${icon.outline}</button><button class="view-button edit-paths" id="editPaths" title="${t("editPaths")}">${icon.edit}</button></div>
        <section class="edit-toolbar" id="editSection" hidden><small id="selectionLabel">${t("noSelection")}</small><i></i><label class="paint-control"><span>${t("fill")}</span><input id="fillColor" type="color" value="#315fce" title="${t("fill")}" disabled></label><button class="paint-none" id="removeFill" title="${t("removeFill")}" disabled>${icon.close}</button><label class="paint-control"><span>${t("stroke")}</span><input id="strokeColor" type="color" value="#252c39" title="${t("stroke")}" disabled></label><button class="paint-none" id="removeStroke" title="${t("removeStroke")}" disabled>${icon.close}</button><i></i><div class="edit-actions"><button id="undoEdit" title="${t("undo")}" disabled>${icon.undo}</button><button id="redoEdit" title="${t("redo")}" disabled>${icon.redo}</button><button id="deleteShape" title="${t("deleteShape")}" disabled>${icon.trash}</button><button class="save-edit" id="saveEdits" title="${t("saveChanges")}" disabled>${icon.save}</button></div></section>
        <div class="status-strip" id="statusStrip"><span class="status-icon">${icon.sparkle}</span><span class="status-copy"><span class="status-heading"><strong id="statusTitle">${t("selectJob")}</strong><small class="status-eta" id="statusEta"></small></span><small id="statusDetail"></small></span><i class="progress" id="progressTrack" role="progressbar" aria-valuemin="0" aria-valuemax="100"><b id="progressFill"></b></i></div>
      </div>
      <div class="result-meta" id="resultMeta"></div>
    </section>
    <aside class="controls">
      <section><span class="label">${t("source")}</span><button class="source-button" id="chooseImages"><span class="source-thumb" id="sourceThumb">${icon.image}</span><span><strong id="sourceName">${t("addImages")}</strong><small id="sourceMeta"></small></span></button></section>
      <section><span class="label">${t("model")}</span><div class="model-control"><button data-model="simple" class="active"><strong>${t("simple")}</strong><small>${t("simpleHint")}</small></button><button data-model="detail"><strong>${t("detail")}</strong><small>${t("detailHint")}</small></button></div></section>
      <section class="compact-setting"><span class="label">${t("transparentBackground")}</span><div class="segmented-control" role="group" aria-label="${t("transparentBackground")}"><button data-background="auto" title="${t("backgroundAutoHint")}">${t("backgroundAuto")}</button><button data-background="transparent" title="${t("backgroundOnHint")}">${t("backgroundOn")}</button><button data-background="opaque" class="active" title="${t("backgroundOffHint")}">${t("backgroundOff")}</button></div></section>
      <section><span class="label">${t("saveTo")}</span><button class="folder-row" id="chooseFolder">${icon.folder}<span id="folderPath"></span></button></section>
      <div class="action-area"><button class="primary" id="generate">${icon.sparkle}<span>${t("generate")}</span></button><button class="secondary" id="cancel">${t("cancel")}</button><button class="secondary" id="openFolder">${icon.folder}<span>${t("openFolder")}</span></button></div>
    </aside>
  </main>
  <div class="app-toast" id="appToast" role="status" aria-live="polite"></div>
</section>`;
}
