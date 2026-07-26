import { t } from "./i18n";
import type { Asset, Item } from "./types";
import { DepthPreviewController } from "./depth-preview";

const EDITABLE_SELECTOR = "path,rect,circle,ellipse,polygon,polyline,line";
const MAX_ANIMATED_PATHS = 120;

type Invoke = <T = unknown>(cmd: string, args?: unknown) => Promise<T>;

type CanvasOptions = {
  getSelected: () => Item | undefined;
  getItems: () => Item[];
  isSelected: (id: string) => boolean;
  busy: (item: Item) => boolean;
  loadSource: (item: Item) => Promise<string>;
  invoke: Invoke;
  imageIcon: string;
  vectorIcon: string;
  holdPreviews: (milliseconds: number) => void;
  setPreviewInteraction: (active: boolean) => void;
};

function query<T extends Element>(selector: string) {
  return document.querySelector<T>(selector)!;
}

function sanitizeSvg(text: string): SVGSVGElement {
  const doc = new DOMParser().parseFromString(text, "image/svg+xml");
  if (doc.querySelector("parsererror") || doc.documentElement.tagName.toLowerCase() !== "svg") {
    throw new Error("Invalid SVG result");
  }
  doc.querySelectorAll("script, foreignObject, iframe, object, embed").forEach((node) => node.remove());
  doc.querySelectorAll("*").forEach((node) => {
    for (const attr of [...node.attributes]) {
      const name = attr.name.toLowerCase();
      const value = attr.value.trim();
      const externalReference = name === "href" || name === "xlink:href" || name === "src";
      if (name.startsWith("on") || (externalReference && !value.startsWith("#") && !value.startsWith("data:"))) {
        node.removeAttribute(attr.name);
      }
    }
  });
  return document.importNode(doc.documentElement, true) as unknown as SVGSVGElement;
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
  private readonly statusStrip = query<HTMLElement>("#statusStrip");
  private readonly resultMeta = query<HTMLElement>("#resultMeta");
  private readonly sourceName = query<HTMLElement>("#sourceName");
  private readonly sourceMeta = query<HTMLElement>("#sourceMeta");
  private readonly sourceThumb = query<HTMLElement>("#sourceThumb");
  private readonly pathAnimationState = new WeakMap<SVGPathElement, {
    stroke: string;
    fillOpacity: string;
    dashArray: string;
    dashOffset: string;
  }>();
  private renderedOutput = "";
  private displayVersion = 0;
  private artboardResizeObserver?: ResizeObserver;
  private readonly depthPreview: DepthPreviewController;
  private activeSvg?: SVGSVGElement;
  private activeViewport?: HTMLElement;
  private selectedShape?: SVGGraphicsElement;
  private viewScale = 1;
  private viewX = 0;
  private viewY = 0;
  private viewBaseWidth = 0;
  private viewBaseHeight = 0;
  private outlineVisible = false;
  private backgroundMode = 0;
  private panStart?: { x: number; y: number; viewX: number; viewY: number };
  private panMoved = false;

  constructor(private readonly options: CanvasOptions) {
    this.depthPreview = new DepthPreviewController({
      artboard: this.artboard,
      isSelected: options.isSelected,
      busy: options.busy,
    });
    this.bindControls();
    this.bindCanvasGestures();
    this.bindKeyboard();
  }

  invalidate() {
    this.renderedOutput = "";
  }

  clear() {
    this.finishPathAnimations();
    this.depthPreview.stop();
    this.artboardResizeObserver?.disconnect();
    this.activeSvg = undefined;
    this.activeViewport = undefined;
    this.viewBaseWidth = 0;
    this.viewBaseHeight = 0;
    this.selectedShape = undefined;
    this.viewerToolbar.hidden = true;
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
    }
  }

  updateResultMeta(item = this.options.getSelected()) {
    if (!item || item.stage !== "done") {
      this.resultMeta.textContent = "";
      return;
    }
    const suffix = item.saveError ? t("saveFailed") : item.dirty ? t("unsaved") : "";
    this.resultMeta.textContent =
      `${item.pathCount ?? 0} ${t("paths")} · ${item.outputName || "SVG"}${suffix ? ` · ${suffix}` : ""}`;
  }

  async showItem(item?: Item, animateSvg = true) {
    if (!item) return;
    const version = ++this.displayVersion;
    const isCurrent = () => version === this.displayVersion && this.options.isSelected(item.id);
    this.sourceName.textContent = item.name;
    this.sourceMeta.textContent = item.model === "detail" ? t("detail") : t("simple");
    this.sourceThumb.innerHTML = item.thumbnailUrl
      ? `<img src="${item.thumbnailUrl}" alt="" />`
      : this.options.imageIcon;
    const source = await this.options.loadSource(item).catch(() => "");
    if (!isCurrent()) {
      item.sourceUrl = undefined;
      return;
    }
    this.options.getItems().forEach((candidate) => {
      if (candidate !== item) candidate.sourceUrl = undefined;
    });
    if (item.stage === "done" && item.outputPath) await this.showSvgResult(item, animateSvg, isCurrent);
    else if (source && item.depthUrl && this.options.busy(item)) {
      const depthKey = `depth:${item.id}:${item.previewPath}`;
      if (this.renderedOutput === depthKey) return;
      this.clear();
      if (await this.depthPreview.show(item)) this.renderedOutput = depthKey;
    } else if (source && this.renderedOutput !== `source:${item.id}`) {
      this.clear();
      this.depthPreview.stop();
      this.artboardResizeObserver?.disconnect();
      this.artboard.innerHTML = `<img class="source-preview" src="${source}" alt="" />`;
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

  private async showSvgResult(item: Item, animateSvg: boolean, isCurrent: () => boolean) {
    if (!item.svgText) {
      const asset = await this.options.invoke<Asset>("read_asset", { path: item.outputPath });
      item.svgText = asset.text;
    }
    if (!isCurrent() || !item.svgText || this.renderedOutput === item.outputPath) return;
    this.depthPreview.stop();
    this.finishPathAnimations();
    const svg = sanitizeSvg(item.svgText);
    if (item.originalWidth === undefined) item.originalWidth = svg.getAttribute("width") || "";
    if (item.originalHeight === undefined) item.originalHeight = svg.getAttribute("height") || "";
    const viewBox = (svg.getAttribute("viewBox") || "").trim().split(/[\s,]+/).map(Number);
    const width = viewBox.length === 4 && viewBox[2] > 0
      ? viewBox[2]
      : Number.parseFloat(svg.getAttribute("width") || "");
    const height = viewBox.length === 4 && viewBox[3] > 0
      ? viewBox[3]
      : Number.parseFloat(svg.getAttribute("height") || "");
    svg.removeAttribute("width");
    svg.removeAttribute("height");
    svg.setAttribute("preserveAspectRatio", "xMidYMid meet");
    svg.setAttribute("role", "img");
    const viewport = document.createElement("div");
    viewport.className = "svg-viewport";
    viewport.append(svg);
    this.artboardResizeObserver?.disconnect();
    this.artboard.replaceChildren(viewport);
    this.activeSvg = svg;
    this.activeViewport = viewport;
    this.selectedShape = undefined;
    const ratio = Number.isFinite(width) && Number.isFinite(height) && width > 0 && height > 0
      ? width / height
      : 1;
    this.fitSvgViewport(viewport, ratio);
    this.resetView();
    this.syncCanvasModes();
    if (item.savedSvgText === undefined) {
      item.savedSvgText = this.serializeActiveSvg(item);
      item.svgText = item.savedSvgText;
    }
    item.pathCount = svg.querySelectorAll("path").length;
    this.renderedOutput = item.outputPath || "";
    this.syncEditorUi();
    if (animateSvg) requestAnimationFrame(() => this.animatePaths(svg));
  }

  private restoreAnimatedPath(path: SVGPathElement) {
    const state = this.pathAnimationState.get(path);
    if (!state) return;
    path.getAnimations().forEach((animation) => animation.cancel());
    const restore = (property: string, value: string) => {
      if (value) path.style.setProperty(property, value);
      else path.style.removeProperty(property);
    };
    restore("stroke", state.stroke);
    restore("fill-opacity", state.fillOpacity);
    restore("stroke-dasharray", state.dashArray);
    restore("stroke-dashoffset", state.dashOffset);
    this.pathAnimationState.delete(path);
  }

  private finishPathAnimations(svg = this.activeSvg) {
    svg?.querySelectorAll<SVGPathElement>("path").forEach((path) => this.restoreAnimatedPath(path));
  }

  private animatePaths(svg: SVGSVGElement) {
    if (matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    const allPaths = [...svg.querySelectorAll<SVGPathElement>("path")];
    const stride = Math.max(1, Math.ceil(allPaths.length / MAX_ANIMATED_PATHS));
    const paths = allPaths.filter((_, index) => index % stride === 0).slice(0, MAX_ANIMATED_PATHS);
    const totalDuration = Math.min(4_200, Math.max(1_600, 1_200 + Math.log2(paths.length + 1) * 320));
    this.options.holdPreviews(totalDuration);
    const delayWindow = totalDuration * 0.3;
    const baseDuration = totalDuration - delayWindow;
    const measurements = paths.flatMap((path) => {
      try {
        const length = path.getTotalLength();
        if (!Number.isFinite(length) || length <= 0) return [];
        return [{ path, length, stroke: getComputedStyle(path).stroke }];
      } catch {
        return [];
      }
    });
    measurements.forEach(({ path, length, stroke }, index) => {
      this.pathAnimationState.set(path, {
        stroke: path.style.stroke,
        fillOpacity: path.style.fillOpacity,
        dashArray: path.style.strokeDasharray,
        dashOffset: path.style.strokeDashoffset,
      });
      path.style.stroke = stroke === "none" ? "var(--ink-accent)" : stroke;
      path.style.strokeDasharray = `${length}`;
      path.style.strokeDashoffset = `${length}`;
      path.style.fillOpacity = "0";
      const animation = path.animate(
        [
          { strokeDashoffset: length, fillOpacity: 0 },
          { strokeDashoffset: 0, fillOpacity: 0, offset: 0.78 },
          { strokeDashoffset: 0, fillOpacity: 1 },
        ],
        {
          duration: baseDuration * (0.72 + Math.min(length / 700, 1) * 0.28),
          delay: measurements.length > 1 ? (index / (measurements.length - 1)) * delayWindow : 0,
          easing: "cubic-bezier(.2,.75,.25,1)",
          fill: "forwards",
        },
      );
      animation.finished.then(() => this.restoreAnimatedPath(path)).catch(() => undefined);
    });
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
    query("#showOutlines").classList.toggle("active", this.outlineVisible);
    query("#canvasBackground").classList.toggle("active", this.backgroundMode !== 1);
  }

  private serializeActiveSvg(item = this.options.getSelected()) {
    if (!this.activeSvg || !item) return item?.svgText || "";
    this.finishPathAnimations(this.activeSvg);
    const clone = this.activeSvg.cloneNode(true) as SVGSVGElement;
    clone.classList.remove("viewer-outlines");
    clone.querySelectorAll(".vector-selected").forEach((element) => element.classList.remove("vector-selected"));
    if (!clone.getAttribute("class")) clone.removeAttribute("class");
    clone.querySelectorAll("[class='']").forEach((element) => element.removeAttribute("class"));
    if (item.originalWidth) clone.setAttribute("width", item.originalWidth);
    else clone.removeAttribute("width");
    if (item.originalHeight) clone.setAttribute("height", item.originalHeight);
    else clone.removeAttribute("height");
    clone.removeAttribute("role");
    return new XMLSerializer().serializeToString(clone);
  }

  private syncEditorUi() {
    const item = this.options.getSelected();
    const editable = item?.stage === "done" && Boolean(this.activeSvg);
    this.viewerToolbar.hidden = !editable;
    this.editSection.hidden = !editable;
    this.statusStrip.hidden = editable;
    const hasSelection = editable && Boolean(this.selectedShape?.isConnected);
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
      const computed = getComputedStyle(this.selectedShape);
      const fill = this.selectedShape.style.fill || this.selectedShape.getAttribute("fill") || computed.fill;
      const stroke = this.selectedShape.style.stroke || this.selectedShape.getAttribute("stroke") || computed.stroke;
      this.fillColor.value = this.colorToHex(fill, "#315fce");
      this.strokeColor.value = this.colorToHex(stroke, "#252c39");
      this.removeFill.classList.toggle("active", fill === "none" || computed.fill === "none");
      this.removeStroke.classList.toggle("active", stroke === "none" || computed.stroke === "none");
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
    this.finishPathAnimations();
    this.selectedShape?.classList.remove("vector-selected");
    this.selectedShape = shape?.isConnected ? shape : undefined;
    this.selectedShape?.classList.add("vector-selected");
    this.syncEditorUi();
  }

  private pushUndo(item: Item) {
    item.undoStack ||= [];
    item.undoStack.push(this.serializeActiveSvg(item));
    if (item.undoStack.length > 50) item.undoStack.shift();
    item.redoStack = [];
  }

  private commitLiveEdit(item: Item) {
    item.svgText = this.serializeActiveSvg(item);
    item.dirty = item.svgText !== item.savedSvgText;
    item.saveError = false;
    item.pathCount = this.activeSvg?.querySelectorAll("path").length || 0;
    this.syncEditorUi();
  }

  private applyPaint(property: "fill" | "stroke", value: string) {
    const item = this.options.getSelected();
    if (!item || !this.selectedShape) return;
    this.pushUndo(item);
    this.selectedShape.style.setProperty(property, value);
    this.commitLiveEdit(item);
  }

  private async restoreEdit(item: Item, svg: string) {
    item.svgText = svg;
    item.dirty = svg !== item.savedSvgText;
    item.saveError = false;
    this.invalidate();
    await this.showItem(item, false);
  }

  private async undoCurrentEdit() {
    const item = this.options.getSelected();
    const previous = item?.undoStack?.pop();
    if (!item || !previous) return;
    item.redoStack ||= [];
    item.redoStack.push(this.serializeActiveSvg(item));
    await this.restoreEdit(item, previous);
  }

  private async redoCurrentEdit() {
    const item = this.options.getSelected();
    const next = item?.redoStack?.pop();
    if (!item || !next) return;
    item.undoStack ||= [];
    item.undoStack.push(this.serializeActiveSvg(item));
    await this.restoreEdit(item, next);
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
      item.savedSvgText = svg;
      item.dirty = false;
      item.saveError = false;
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
      this.pushUndo(item);
      this.selectedShape.remove();
      this.selectedShape = undefined;
      this.commitLiveEdit(item);
    });
    this.saveEdits.addEventListener("click", () => void this.saveCurrentEdits());
  }

  private bindCanvasGestures() {
    this.artboard.addEventListener("wheel", (event) => {
      if (!this.activeViewport) return;
      this.options.holdPreviews(220);
      event.preventDefault();
      const bounds = this.artboard.getBoundingClientRect();
      const anchor = {
        x: event.clientX - bounds.left - bounds.width / 2,
        y: event.clientY - bounds.top - bounds.height / 2,
      };
      this.setZoom(this.viewScale * (event.deltaY < 0 ? 1.12 : 0.89), anchor);
    }, { passive: false });
    this.artboard.addEventListener("dblclick", (event) => {
      if (!this.activeViewport || (event.target as Element).closest(EDITABLE_SELECTOR)) return;
      this.resetView();
    });
    this.artboard.addEventListener("pointerdown", (event) => {
      if (!this.activeViewport || event.button !== 0) return;
      this.options.setPreviewInteraction(true);
      this.panStart = { x: event.clientX, y: event.clientY, viewX: this.viewX, viewY: this.viewY };
      this.panMoved = false;
      this.artboard.setPointerCapture(event.pointerId);
    });
    this.artboard.addEventListener("pointermove", (event) => {
      if (!this.panStart) return;
      const dx = event.clientX - this.panStart.x;
      const dy = event.clientY - this.panStart.y;
      if (Math.abs(dx) + Math.abs(dy) > 3) this.panMoved = true;
      if (!this.panMoved) return;
      this.viewX = this.panStart.viewX + dx;
      this.viewY = this.panStart.viewY + dy;
      this.artboard.classList.add("is-panning");
      this.applyViewTransform();
    });
    this.artboard.addEventListener("pointerup", (event) => this.finishPan(event.pointerId));
    this.artboard.addEventListener("pointercancel", (event) => this.finishPan(event.pointerId, true));
    this.artboard.addEventListener("click", (event) => {
      if (this.panMoved || !this.activeSvg) return;
      const target = (event.target as Element).closest(EDITABLE_SELECTOR) as SVGGraphicsElement | null;
      this.selectShape(target && this.activeSvg.contains(target) ? target : undefined);
    });
  }

  private finishPan(pointerId: number, cancelled = false) {
    if (!this.panStart) return;
    this.panStart = undefined;
    this.artboard.classList.remove("is-panning");
    if (this.artboard.hasPointerCapture(pointerId)) this.artboard.releasePointerCapture(pointerId);
    this.options.setPreviewInteraction(false);
    if (cancelled) this.panMoved = false;
    else if (this.panMoved) setTimeout(() => {
      this.panMoved = false;
    }, 0);
  }

  private bindKeyboard() {
    window.addEventListener("keydown", (event) => {
      if (event.target instanceof Element && event.target.closest("input")) return;
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "s") {
        event.preventDefault();
        void this.saveCurrentEdits();
      } else if ((event.ctrlKey || event.metaKey) && event.key === "0") {
        event.preventDefault();
        this.resetView();
      } else if ((event.ctrlKey || event.metaKey) && (event.key === "+" || event.key === "=")) {
        event.preventDefault();
        this.setZoom(this.viewScale * 1.2);
      } else if ((event.ctrlKey || event.metaKey) && event.key === "-") {
        event.preventDefault();
        this.setZoom(this.viewScale / 1.2);
      } else if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "z") {
        event.preventDefault();
        void (event.shiftKey ? this.redoCurrentEdit() : this.undoCurrentEdit());
      } else if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "y") {
        event.preventDefault();
        void this.redoCurrentEdit();
      } else if ((event.key === "Delete" || event.key === "Backspace") && this.selectedShape) {
        event.preventDefault();
        this.deleteShape.click();
      } else if (event.key === "Escape" && this.selectedShape) {
        this.selectShape();
      }
    });
  }
}
