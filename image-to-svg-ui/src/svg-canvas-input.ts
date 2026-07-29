type ViewState = { scale: number; x: number; y: number };

export type PointerSelectionResolution = {
  apply: boolean;
  index?: number;
};

export function resolvePointerSelection(
  index: number | undefined,
  moved: boolean,
  cancelled: boolean,
): PointerSelectionResolution {
  return moved || cancelled ? { apply: false } : { apply: true, index };
}

function editableIndexFromEvent(event: Event): number | undefined {
  for (const target of event.composedPath()) {
    if (!(target instanceof Element)) continue;
    const value = Number(
      target.closest<SVGGraphicsElement>("[data-edit-index]")?.dataset.editIndex,
    );
    if (Number.isInteger(value)) return value;
  }
  return undefined;
}

type GestureOptions = {
  artboard: HTMLElement;
  hasViewport: () => boolean;
  getView: () => ViewState;
  setView: (x: number, y: number) => void;
  applyView: () => void;
  setZoom: (scale: number, anchor?: { x: number; y: number }) => void;
  resetView: () => void;
  selectIndex: (index: number | undefined) => void;
};

export function bindSvgCanvasGestures(options: GestureOptions) {
  const { artboard } = options;
  let panStart: {
    x: number;
    y: number;
    viewX: number;
    viewY: number;
    editIndex?: number;
  } | undefined;
  let panMoved = false;

  const finishPan = (pointerId: number, cancelled = false) => {
    if (!panStart) return;
    const selection = resolvePointerSelection(panStart.editIndex, panMoved, cancelled);
    panStart = undefined;
    artboard.classList.remove("is-panning");
    if (artboard.hasPointerCapture(pointerId)) artboard.releasePointerCapture(pointerId);
    panMoved = false;
    if (selection.apply) options.selectIndex(selection.index);
  };

  artboard.addEventListener("wheel", (event) => {
    if (!options.hasViewport()) return;
    event.preventDefault();
    const bounds = artboard.getBoundingClientRect();
    const anchor = {
      x: event.clientX - bounds.left - bounds.width / 2,
      y: event.clientY - bounds.top - bounds.height / 2,
    };
    options.setZoom(options.getView().scale * (event.deltaY < 0 ? 1.12 : 0.89), anchor);
  }, { passive: false });
  artboard.addEventListener("dblclick", (event) => {
    if (!options.hasViewport() || editableIndexFromEvent(event) !== undefined) return;
    options.resetView();
  });
  artboard.addEventListener("pointerdown", (event) => {
    if (!options.hasViewport() || event.button !== 0) return;
    const view = options.getView();
    panStart = {
      x: event.clientX,
      y: event.clientY,
      viewX: view.x,
      viewY: view.y,
      editIndex: editableIndexFromEvent(event),
    };
    panMoved = false;
  });
  artboard.addEventListener("pointermove", (event) => {
    if (!panStart) return;
    const dx = event.clientX - panStart.x;
    const dy = event.clientY - panStart.y;
    if (Math.abs(dx) + Math.abs(dy) > 3 && !panMoved) {
      panMoved = true;
      artboard.setPointerCapture(event.pointerId);
    }
    if (!panMoved) return;
    event.preventDefault();
    options.setView(panStart.viewX + dx, panStart.viewY + dy);
    artboard.classList.add("is-panning");
    options.applyView();
  });
  artboard.addEventListener("pointerup", (event) => finishPan(event.pointerId));
  artboard.addEventListener("pointercancel", (event) => finishPan(event.pointerId, true));
}

type KeyboardOptions = {
  save: () => void;
  resetView: () => void;
  zoomBy: (factor: number) => void;
  undo: () => void;
  redo: () => void;
  hasSelection: () => boolean;
  deleteSelection: () => void;
  clearSelection: () => void;
};

export function bindSvgCanvasKeyboard(options: KeyboardOptions) {
  window.addEventListener("keydown", (event) => {
    if (event.target instanceof Element && event.target.closest("input")) return;
    const modifier = event.ctrlKey || event.metaKey;
    const key = event.key.toLowerCase();
    if (modifier && key === "s") {
      event.preventDefault();
      options.save();
    } else if (modifier && event.key === "0") {
      event.preventDefault();
      options.resetView();
    } else if (modifier && (event.key === "+" || event.key === "=")) {
      event.preventDefault();
      options.zoomBy(1.2);
    } else if (modifier && event.key === "-") {
      event.preventDefault();
      options.zoomBy(1 / 1.2);
    } else if (modifier && key === "z") {
      event.preventDefault();
      (event.shiftKey ? options.redo : options.undo)();
    } else if (modifier && key === "y") {
      event.preventDefault();
      options.redo();
    } else if ((event.key === "Delete" || event.key === "Backspace") && options.hasSelection()) {
      event.preventDefault();
      options.deleteSelection();
    } else if (event.key === "Escape" && options.hasSelection()) {
      options.clearSelection();
    }
  });
}
