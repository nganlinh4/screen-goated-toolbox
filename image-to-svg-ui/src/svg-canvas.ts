import { t } from "./i18n";
import type { Item } from "./types";
import { applyDelta, deltaBytes, elementPath, pushUndo } from "./svg-edit-history";
import { bindSvgCanvasGestures, bindSvgCanvasKeyboard } from "./svg-canvas-input";
import {
  EDITABLE_SELECTOR,
  EDIT_SURFACE_STYLE,
  prepareEditableSvg,
} from "./svg-edit-surface";
import {
  shouldConstructEditableSurface,
  type SvgSurfaceIntent,
} from "./svg-display-policy";
import { LatestOnlyLane } from "../../ui-shared/latest-only-lane";

type Invoke = <T = unknown>(cmd: string, args?: unknown) => Promise<T>;

type CanvasOptions = {
  getSelected: () => Item | undefined;
  getItems: () => Item[];
  isSelected: (id: string) => boolean;
  busy: (item: Item) => boolean;
  loadSource: (item: Item) => Promise<string>;
  loadVectorPreview: (item: Item) => Promise<string>;
  loadVectorText: (item: Item) => Promise<string>;
  cacheVector: (item: Item, svg: string) => void;
  invalidateVectorPreview: (item: Item) => void;
  invoke: Invoke;
  imageIcon: string;
  vectorIcon: string;
};

function query<T extends Element>(selector: string) {
  return document.querySelector<T>(selector)!;
}

export class SvgCanvasController {
  private readonly artboard = query<HTMLElement>("#artboard");
  private readonly viewerToolbar = query<HTMLElement>("#viewerToolbar");
  private readonly zoomValue = query<HTMLOutputElement>("#zoomValue");
  private readonly editSection = query<HTMLElement>("#editSection");
  private readonly selectionLabel = query<HTMLElement>("#selectionLabel");
  private readonly fillColor = query<HTMLInputElement>("#fillColor");
  private readonly strokeColor = query<HTMLInputElement>("#strokeColor");
  private readonly removeFill = query<HTMLButtonElement>("#removeFill");
  private readonly removeStroke = query<HTMLButtonElement>("#removeStroke");
  private readonly undoEdit = query<HTMLButtonElement>("#undoEdit");
  private readonly redoEdit = query<HTMLButtonElement>("#redoEdit");
  private readonly deleteShape = query<HTMLButtonElement>("#deleteShape");
  private readonly saveEdits = query<HTMLButtonElement>("#saveEdits");
  private readonly editPaths = query<HTMLButtonElement>("#editPaths");
  private readonly statusStrip = query<HTMLElement>("#statusStrip");
  private readonly resultMeta = query<HTMLElement>("#resultMeta");
  private readonly sourceName = query<HTMLElement>("#sourceName");
  private readonly sourceMeta = query<HTMLElement>("#sourceMeta");
  private readonly sourceThumb = query<HTMLElement>("#sourceThumb");
  private renderedOutput = "";
  private renderedSvgObjectUrl = "";
  private displayVersion = 0;
  private readonly displayLane = new LatestOnlyLane<void>();
  private artboardResizeObserver?: ResizeObserver;
  private activeSvg?: SVGSVGElement;
  private activeViewport?: HTMLElement;
  private selectedShape?: SVGGraphicsElement;
  private selectionOverlay?: SVGSVGElement;
  private viewScale = 1;
  private viewX = 0;
  private viewY = 0;
  private viewBaseWidth = 0;
  private viewBaseHeight = 0;
  private outlineVisible = false;
  private backgroundMode = 0;
  private editingEnabled = false;
  private surfaceIntent: SvgSurfaceIntent = "preview";
  private pendingRenderFrame = 0;

  constructor(private readonly options: CanvasOptions) {
    this.bindControls();
    bindSvgCanvasGestures({
      artboard: this.artboard,
      hasViewport: () => Boolean(this.activeViewport),
      getView: () => ({ scale: this.viewScale, x: this.viewX, y: this.viewY }),
      setView: (x, y) => {
        this.viewX = x;
        this.viewY = y;
      },
      applyView: () => this.applyViewTransform(),
      setZoom: (scale, anchor) => this.setZoom(scale, anchor),
      resetView: () => this.resetView(),
      selectIndex: (index) => this.selectShape(index === undefined
        ? undefined
        : [...(this.activeSvg?.querySelectorAll<SVGGraphicsElement>(EDITABLE_SELECTOR) || [])][index]),
    });
    bindSvgCanvasKeyboard({
      save: () => void this.saveCurrentEdits(),
      resetView: () => this.resetView(),
      zoomBy: (factor) => this.setZoom(this.viewScale * factor),
      undo: () => void this.undoCurrentEdit(),
      redo: () => void this.redoCurrentEdit(),
      hasSelection: () => Boolean(this.selectedShape),
      deleteSelection: () => this.deleteShape.click(),
      clearSelection: () => this.selectShape(),
    });
  }

  invalidate() {
    this.displayVersion += 1;
    this.displayLane.invalidate();
    this.renderedOutput = "";
    this.surfaceIntent = "preview";
  }

  clear() {
    if (this.pendingRenderFrame) cancelAnimationFrame(this.pendingRenderFrame);
    this.pendingRenderFrame = 0;
    if (this.renderedSvgObjectUrl) URL.revokeObjectURL(this.renderedSvgObjectUrl);
    this.renderedSvgObjectUrl = "";
    this.artboardResizeObserver?.disconnect();
    this.activeSvg = undefined;
    this.activeViewport = undefined;
    this.viewBaseWidth = 0;
    this.viewBaseHeight = 0;
    this.selectedShape = undefined;
    this.selectionOverlay = undefined;
    this.editingEnabled = false;
    this.surfaceIntent = "preview";
    this.viewerToolbar.hidden = true;
    this.editPaths.hidden = true;
    this.editPaths.classList.remove("active");
    this.editSection.hidden = true;
    this.statusStrip.hidden = false;
    this.artboard.classList.remove("is-panning");
  }

  showEmpty() {
    this.clear();
    this.invalidate();
    this.artboard.innerHTML = `<div class="empty-state">${this.options.vectorIcon}<strong>${t("canvasEmpty")}</strong><span>${t("canvasHint")}</span></div>`;
  }

  syncEditorVisibility(item?: Item) {
    if (!item || item.stage !== "done") {
      this.viewerToolbar.hidden = true;
      this.editSection.hidden = true;
      this.editPaths.hidden = true;
    }
  }

  updateResultMeta(item = this.options.getSelected()) {
    if (!item || item.stage !== "done") {
      this.resultMeta.textContent = "";
      return;
    }
    const suffix = item.editingUnavailable
      ? t("editingUnavailable")
      : item.editLimitReached
        ? t("editLimitReached")
      : item.saveError
        ? t("saveFailed")
        : item.dirty
          ? t("unsaved")
          : "";
    const pathCount = item.pathCount === undefined ? "" : `${item.pathCount} ${t("paths")} · `;
    this.resultMeta.textContent =
      `${pathCount}${item.outputName || "SVG"}${suffix ? ` · ${suffix}` : ""}`;
  }

  async showItem(item?: Item, _animateSvg = true) {
    if (!item) return;
    const version = ++this.displayVersion;
    this.sourceName.textContent = item.name;
    this.sourceMeta.textContent = item.model === "detail" ? t("detail") : t("simple");
    this.sourceThumb.innerHTML = this.options.imageIcon;
    await this.displayLane.run(
      (signal) => this.showLatestItem(item, version, signal),
      () => undefined,
    );
  }

  private async showLatestItem(
    item: Item,
    version: number,
    signal: AbortSignal,
  ) {
    const isCurrent = () => !signal.aborted
      && version === this.displayVersion
      && this.options.isSelected(item.id);
    const hasResult = item.stage === "done" && Boolean(item.outputPath);
    const source = hasResult ? "" : await this.options.loadSource(item).catch(() => "");
    if (!isCurrent()) return;
    this.options.getItems().forEach((candidate) => {
      if (candidate === item) return;
      if (!candidate.dirty) {
        candidate.svgText = undefined;
        candidate.undoStack = undefined;
        candidate.redoStack = undefined;
        candidate.undoBytes = 0;
        candidate.redoBytes = 0;
      }
    });
    if (hasResult) await this.showSvgResult(item, isCurrent);
    else if (source && this.renderedOutput !== `source:${item.id}`) {
      this.clear();
      this.artboardResizeObserver?.disconnect();
      this.artboard.innerHTML =
        `<img class="source-preview" src="${source}" decoding="async" alt="" />`;
      this.renderedOutput = `source:${item.id}`;
    }
  }

  setZoom(next: number, anchor = { x: 0, y: 0 }) {
    const scale = Math.min(8, Math.max(0.25, next));
    const contentX = (anchor.x - this.viewX) / this.viewScale;
    const contentY = (anchor.y - this.viewY) / this.viewScale;
    this.viewX = anchor.x - contentX * scale;
    this.viewY = anchor.y - contentY * scale;
    this.viewScale = scale;
    this.applyViewTransform();
  }

  resetView() {
    this.viewScale = 1;
    this.viewX = 0;
    this.viewY = 0;
    this.applyViewTransform();
  }

  selectFirstShape() {
    this.selectShape(this.activeSvg?.querySelector<SVGGraphicsElement>(EDITABLE_SELECTOR) || undefined);
  }

  private async showSvgResult(item: Item, isCurrent: () => boolean) {
    if (!item.svgText && !item.svgPreviewUrl) {
      item.svgPreviewUrl = await this.options.loadVectorPreview(item);
    }
    const source = item.svgText || item.svgPreviewUrl;
    if (!isCurrent() || !source || this.renderedOutput === item.outputPath) return;
    this.surfaceIntent = "preview";
    this.editingEnabled = false;
    this.activeSvg = undefined;
    const viewport = document.createElement("div");
    viewport.className = "svg-viewport";
    this.artboardResizeObserver?.disconnect();
    this.artboard.replaceChildren(viewport);
    this.activeViewport = viewport;
    this.selectedShape = undefined;
    this.fitSvgViewport(viewport, 1);
    this.resetView();
    this.syncCanvasModes();
    this.renderStaticSvg(source, Boolean(item.svgText));
    this.renderedOutput = item.outputPath || "";
    this.syncEditorUi();
  }

  private async activateEditing() {
    const item = this.options.getSelected();
    this.surfaceIntent = "edit";
    if (!item || !shouldConstructEditableSurface(this.surfaceIntent, item.stage, item.outputPath)) {
      this.surfaceIntent = "preview";
      return;
    }
    this.editPaths.disabled = true;
    try {
      if (!item.svgText) item.svgText = await this.options.loadVectorText(item);
      if (this.options.getSelected()?.id !== item.id || !item.svgText) return;
      const surface = prepareEditableSvg(item.svgText);
      const { svg } = surface;
      item.editingUnavailable = false;
      this.editingEnabled = true;
      if (item.originalWidth === undefined) item.originalWidth = surface.originalWidth;
      if (item.originalHeight === undefined) item.originalHeight = surface.originalHeight;
      svg.removeAttribute("width");
      svg.removeAttribute("height");
      svg.setAttribute("preserveAspectRatio", "xMidYMid meet");
      this.activeSvg = svg;
      const viewport = document.createElement("div");
      viewport.className = "svg-viewport";
      this.artboardResizeObserver?.disconnect();
      this.artboard.replaceChildren(viewport);
      this.activeViewport = viewport;
      this.selectedShape = undefined;
      this.fitSvgViewport(viewport, surface.ratio);
      this.resetView();
      this.syncCanvasModes();
      item.pathCount = surface.pathCount;
      this.renderEditableSvg();
    } catch {
      this.surfaceIntent = "preview";
      this.editingEnabled = false;
      this.activeSvg = undefined;
      item.editingUnavailable = true;
    }
    if (this.options.getSelected()?.id === item.id) this.syncEditorUi();
  }

  private renderActiveSvg(item: Item) {
    if (!this.activeSvg || !this.activeViewport) return;
    item.svgText = this.serializeActiveSvg(item);
    this.syncOverlaySelection();
  }

  private renderEditableSvg() {
    if (!this.activeViewport) return;
    if (!this.editingEnabled || !this.activeSvg) return;
    if (this.renderedSvgObjectUrl) URL.revokeObjectURL(this.renderedSvgObjectUrl);
    this.renderedSvgObjectUrl = "";
    const shapes = [...this.activeSvg.querySelectorAll<SVGGraphicsElement>(EDITABLE_SELECTOR)];
    this.activeSvg.classList.add("svg-edit-surface");
    shapes.forEach((shape, index) => {
      shape.dataset.editIndex = String(index);
    });
    this.selectionOverlay = this.activeSvg;
    const shadow = this.activeViewport.attachShadow({ mode: "open" });
    const style = document.createElement("style");
    style.textContent = EDIT_SURFACE_STYLE;
    shadow.replaceChildren(style, this.activeSvg);
    this.syncOverlaySelection();
  }

  private renderStaticSvg(source: string, isDocumentText: boolean) {
    if (!this.activeViewport) return;
    if (this.renderedSvgObjectUrl) URL.revokeObjectURL(this.renderedSvgObjectUrl);
    const nextUrl = isDocumentText
      ? URL.createObjectURL(new Blob([source], { type: "image/svg+xml" }))
      : source;
    this.renderedSvgObjectUrl = isDocumentText ? nextUrl : "";
    const image = document.createElement("img");
    image.className = "svg-render-image";
    image.alt = "";
    image.decoding = "async";
    image.src = nextUrl;
    image.addEventListener("load", () => {
      if (!image.isConnected || !this.activeViewport) return;
      const ratio = image.naturalWidth > 0 && image.naturalHeight > 0
        ? image.naturalWidth / image.naturalHeight
        : 1;
      this.fitSvgViewport(this.activeViewport, ratio);
    }, { once: true });
    image.addEventListener("error", () => {
      if (!isDocumentText || this.renderedSvgObjectUrl !== nextUrl) return;
      URL.revokeObjectURL(this.renderedSvgObjectUrl);
      this.renderedSvgObjectUrl = "";
    }, { once: true });
    this.selectionOverlay = undefined;
    this.activeViewport.replaceChildren(image);
  }

  private syncOverlaySelection() {
    const shapes = [...this.activeSvg?.querySelectorAll<SVGGraphicsElement>(EDITABLE_SELECTOR) || []];
    const index = this.selectedShape ? shapes.indexOf(this.selectedShape) : -1;
    this.selectionOverlay
      ?.querySelectorAll("[data-edit-index]")
      .forEach((shape) => shape.classList.toggle("selected", shape.getAttribute("data-edit-index") === String(index)));
  }

  private applyViewTransform() {
    if (!this.activeViewport) return;
    const width = this.viewBaseWidth * this.viewScale;
    const height = this.viewBaseHeight * this.viewScale;
    this.activeViewport.style.width = `${width}px`;
    this.activeViewport.style.height = `${height}px`;
    this.activeViewport.style.left = `${Math.round((this.artboard.clientWidth - width) / 2 + this.viewX)}px`;
    this.activeViewport.style.top = `${Math.round((this.artboard.clientHeight - height) / 2 + this.viewY)}px`;
    this.zoomValue.value = `${Math.round(this.viewScale * 100)}%`;
  }

  private syncCanvasModes() {
    this.artboard.classList.toggle("background-checker", this.backgroundMode === 0);
    this.artboard.classList.toggle("background-light", this.backgroundMode === 1);
    this.artboard.classList.toggle("background-dark", this.backgroundMode === 2);
    this.activeSvg?.classList.toggle("viewer-outlines", this.outlineVisible);
    this.selectionOverlay?.classList.toggle("show-outlines", this.outlineVisible);
    query("#showOutlines").classList.toggle("active", this.outlineVisible);
    query("#canvasBackground").classList.toggle("active", this.backgroundMode !== 1);
  }

  private serializeActiveSvg(item = this.options.getSelected()) {
    if (!this.activeSvg || !item) return item?.svgText || "";
    const clone = this.activeSvg.cloneNode(true) as SVGSVGElement;
    clone.classList.remove("viewer-outlines", "show-outlines", "svg-edit-surface");
    clone.querySelectorAll("[data-edit-index]").forEach((element) => {
      element.removeAttribute("data-edit-index");
      element.classList.remove("selected");
    });
    if (!clone.getAttribute("class")) clone.removeAttribute("class");
    clone.querySelectorAll("[class='']").forEach((element) => element.removeAttribute("class"));
    if (item.originalWidth) clone.setAttribute("width", item.originalWidth);
    else clone.removeAttribute("width");
    if (item.originalHeight) clone.setAttribute("height", item.originalHeight);
    else clone.removeAttribute("height");
    return new XMLSerializer().serializeToString(clone);
  }

  private syncEditorUi() {
    const item = this.options.getSelected();
    const rendered = item?.stage === "done" && Boolean(this.activeViewport);
    const editable = rendered && this.editingEnabled && Boolean(this.activeSvg);
    this.viewerToolbar.hidden = !rendered;
    this.editPaths.hidden = !rendered;
    this.editPaths.disabled = !rendered || editable || Boolean(item?.editingUnavailable);
    this.editPaths.classList.toggle("active", editable);
    this.editSection.hidden = !editable;
    this.statusStrip.hidden = editable;
    const hasSelection = editable
      && Boolean(this.selectedShape && this.activeSvg?.contains(this.selectedShape));
    this.selectionLabel.textContent = hasSelection && this.activeSvg && this.selectedShape
      ? t("shapeSelected", {
        count: [...this.activeSvg.querySelectorAll(EDITABLE_SELECTOR)].indexOf(this.selectedShape) + 1,
      })
      : t("noSelection");
    [this.fillColor, this.strokeColor, this.removeFill, this.removeStroke, this.deleteShape]
      .forEach((control) => control.disabled = !hasSelection);
    this.undoEdit.disabled = !item?.undoStack?.length;
    this.redoEdit.disabled = !item?.redoStack?.length;
    this.saveEdits.disabled = !item?.dirty || !item.outputPath;
    this.saveEdits.classList.toggle("dirty", Boolean(item?.dirty));
    if (hasSelection && this.selectedShape) {
      const fill = this.selectedShape.style.fill || this.selectedShape.getAttribute("fill") || "none";
      const stroke = this.selectedShape.style.stroke || this.selectedShape.getAttribute("stroke") || "none";
      this.fillColor.value = this.colorToHex(fill, "#315fce");
      this.strokeColor.value = this.colorToHex(stroke, "#252c39");
      this.removeFill.classList.toggle("active", fill === "none");
      this.removeStroke.classList.toggle("active", stroke === "none");
    } else {
      this.removeFill.classList.remove("active");
      this.removeStroke.classList.remove("active");
    }
    this.updateResultMeta(item);
  }

  private colorToHex(value: string, fallback: string) {
    if (/^#[0-9a-f]{6}$/i.test(value)) return value;
    if (/^#[0-9a-f]{3}$/i.test(value)) {
      return `#${value.slice(1).split("").map((part) => part + part).join("")}`;
    }
    const match = value.match(/rgba?\(\s*(\d+)[, ]+\s*(\d+)[, ]+\s*(\d+)/i);
    return match
      ? `#${match.slice(1, 4).map((part) => Number(part).toString(16).padStart(2, "0")).join("")}`
      : fallback;
  }

  private selectShape(shape?: SVGGraphicsElement) {
    this.selectedShape = shape && this.activeSvg?.contains(shape) ? shape : undefined;
    this.syncOverlaySelection();
    this.syncEditorUi();
  }

  private commitLiveEdit(item: Item) {
    item.saveError = false;
    item.dirty = true;
    this.syncEditorUi();
    if (this.pendingRenderFrame) return;
    this.pendingRenderFrame = requestAnimationFrame(() => {
      this.pendingRenderFrame = 0;
      if (this.options.getSelected() !== item || !this.activeSvg) return;
      item.svgText = this.serializeActiveSvg(item);
      item.dirty = Boolean(item.undoBaselineLost || item.undoStack?.length);
      item.pathCount = this.activeSvg.querySelectorAll("path").length;
      this.renderActiveSvg(item);
      this.syncEditorUi();
    });
  }

  private applyPaint(property: "fill" | "stroke", value: string) {
    const item = this.options.getSelected();
    if (!item || !this.selectedShape) return;
    const shapePath = elementPath(this.activeSvg!, this.selectedShape);
    const before = this.selectedShape.style.getPropertyValue(property);
    if (!pushUndo(this.options.getItems(), item, {
      kind: "paint", shapePath, property, before, after: value,
    })) {
      item.editLimitReached = true;
      this.syncEditorUi();
      return;
    }
    this.selectedShape.style.setProperty(property, value);
    this.commitLiveEdit(item);
  }

  private undoCurrentEdit() {
    const item = this.options.getSelected();
    const delta = item?.undoStack?.pop();
    if (!item || !delta || !this.activeSvg) return;
    const result = applyDelta(this.activeSvg, delta, false);
    if (!result.applied) return;
    this.selectedShape = result.selected;
    item.redoStack ||= [];
    item.redoStack.push(delta);
    item.undoBytes = Math.max(0, (item.undoBytes || 0) - deltaBytes(delta));
    item.redoBytes = (item.redoBytes || 0) + deltaBytes(delta);
    this.commitLiveEdit(item);
  }

  private redoCurrentEdit() {
    const item = this.options.getSelected();
    const delta = item?.redoStack?.pop();
    if (!item || !delta || !this.activeSvg) return;
    const result = applyDelta(this.activeSvg, delta, true);
    if (!result.applied) return;
    this.selectedShape = result.selected;
    item.undoStack ||= [];
    item.undoStack.push(delta);
    item.redoBytes = Math.max(0, (item.redoBytes || 0) - deltaBytes(delta));
    item.undoBytes = (item.undoBytes || 0) + deltaBytes(delta);
    this.commitLiveEdit(item);
  }

  private async saveCurrentEdits() {
    const item = this.options.getSelected();
    if (!item?.dirty || !item.outputPath) return;
    const svg = this.serializeActiveSvg(item);
    this.saveEdits.classList.add("saving");
    this.saveEdits.disabled = true;
    try {
      await this.options.invoke("save_svg_edits", { path: item.outputPath, svg });
      item.svgText = svg;
      item.svgPreviewUrl = undefined;
      this.options.cacheVector(item, svg);
      this.options.invalidateVectorPreview(item);
      item.dirty = false;
      item.saveError = false;
      item.undoStack = [];
      item.redoStack = [];
      item.undoBytes = 0;
      item.redoBytes = 0;
      item.undoBaselineLost = false;
      item.editLimitReached = false;
    } catch {
      item.saveError = true;
    } finally {
      this.saveEdits.classList.remove("saving");
      this.syncEditorUi();
    }
  }

  private fitSvgViewport(element: HTMLElement, ratio: number) {
    this.artboardResizeObserver?.disconnect();
    const fit = () => {
      const maxWidth = this.artboard.clientWidth * 0.88;
      const maxHeight = this.artboard.clientHeight * 0.82;
      this.viewBaseWidth = Math.min(maxWidth, maxHeight * ratio);
      this.viewBaseHeight = this.viewBaseWidth / ratio;
      this.applyViewTransform();
    };
    this.activeViewport = element;
    this.artboardResizeObserver = new ResizeObserver(fit);
    this.artboardResizeObserver.observe(this.artboard);
    fit();
  }

  private bindControls() {
    query("#zoomOut").addEventListener("click", () => this.setZoom(this.viewScale / 1.2));
    query("#zoomIn").addEventListener("click", () => this.setZoom(this.viewScale * 1.2));
    query("#fitView").addEventListener("click", () => this.resetView());
    query("#canvasBackground").addEventListener("click", () => {
      this.backgroundMode = (this.backgroundMode + 1) % 3;
      this.syncCanvasModes();
    });
    query("#showOutlines").addEventListener("click", () => {
      this.outlineVisible = !this.outlineVisible;
      this.syncCanvasModes();
    });
    this.fillColor.addEventListener("change", () => this.applyPaint("fill", this.fillColor.value));
    this.strokeColor.addEventListener("change", () => this.applyPaint("stroke", this.strokeColor.value));
    this.removeFill.addEventListener("click", () => this.applyPaint("fill", "none"));
    this.removeStroke.addEventListener("click", () => this.applyPaint("stroke", "none"));
    this.undoEdit.addEventListener("click", () => void this.undoCurrentEdit());
    this.redoEdit.addEventListener("click", () => void this.redoCurrentEdit());
    this.deleteShape.addEventListener("click", () => {
      const item = this.options.getSelected();
      if (!item || !this.selectedShape) return;
      const parent = this.selectedShape.parentElement;
      if (!parent) return;
      if (!pushUndo(this.options.getItems(), item, {
        kind: "delete",
        parentPath: elementPath(this.activeSvg!, parent),
        childIndex: [...parent.children].indexOf(this.selectedShape),
        markup: this.selectedShape.outerHTML,
      })) {
        item.editLimitReached = true;
        this.syncEditorUi();
        return;
      }
      this.selectedShape.remove();
      this.selectedShape = undefined;
      this.commitLiveEdit(item);
    });
    this.saveEdits.addEventListener("click", () => void this.saveCurrentEdits());
    this.editPaths.addEventListener("click", () => void this.activateEditing());
  }

}
