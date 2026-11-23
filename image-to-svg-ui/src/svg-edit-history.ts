import type { Item, SvgEditDelta } from "./types";

const MAX_EDIT_HISTORY_BYTES = 2 * 1024 * 1024;
const MAX_GLOBAL_UNSAVED_BYTES = 8 * 1024 * 1024;
const encoder = new TextEncoder();

export function deltaBytes(delta: SvgEditDelta) {
  return encoder.encode(JSON.stringify(delta)).byteLength;
}

export function unsavedEditBytes(items: Item[]) {
  return items.reduce((total, item) => {
    if (!item.dirty) return total;
    return total
      + encoder.encode(item.svgText || "").byteLength
      + (item.undoBytes || 0)
      + (item.redoBytes || 0);
  }, 0);
}

export function elementPath(root: SVGSVGElement, element: Element) {
  const path: number[] = [];
  let current: Element | null = element;
  while (current && current !== root) {
    const parent: Element | null = current.parentElement;
    if (!parent) return [];
    path.unshift([...parent.children].indexOf(current));
    current = parent;
  }
  return path;
}

export function elementAt(root: SVGSVGElement, path: number[]) {
  let current: Element | undefined = root;
  for (const index of path) current = current?.children.item(index) || undefined;
  return current;
}

export function pushUndo(items: Item[], item: Item, delta: SvgEditDelta) {
  const bytes = deltaBytes(delta);
  const firstDirtyCopy = item.dirty ? 0 : encoder.encode(item.svgText || "").byteLength;
  if (unsavedEditBytes(items) + firstDirtyCopy + bytes > MAX_GLOBAL_UNSAVED_BYTES) return false;
  item.undoStack ||= [];
  item.undoStack.push(delta);
  item.undoBytes = (item.undoBytes || 0) + bytes;
  while (item.undoStack.length > 64 || item.undoBytes > MAX_EDIT_HISTORY_BYTES) {
    const removed = item.undoStack.shift();
    item.undoBytes -= removed ? deltaBytes(removed) : 0;
    item.undoBaselineLost = true;
  }
  item.redoStack = [];
  item.redoBytes = 0;
  item.editLimitReached = false;
  return true;
}

export function applyDelta(
  root: SVGSVGElement,
  delta: SvgEditDelta,
  forward: boolean,
): { applied: boolean; selected?: SVGGraphicsElement } {
  if (delta.kind === "paint") {
    const shape = elementAt(root, delta.shapePath) as SVGGraphicsElement | undefined;
    if (!shape) return { applied: false };
    const value = forward ? delta.after : delta.before;
    if (value) shape.style.setProperty(delta.property, value);
    else shape.style.removeProperty(delta.property);
    return { applied: true, selected: shape };
  }
  const parent = elementAt(root, delta.parentPath);
  if (!parent) return { applied: false };
  if (forward) {
    parent.children.item(delta.childIndex)?.remove();
    return { applied: true };
  }
  const doc = new DOMParser().parseFromString(
    `<svg xmlns="http://www.w3.org/2000/svg">${delta.markup}</svg>`,
    "image/svg+xml",
  );
  const restored = document.importNode(doc.documentElement.firstElementChild!, true);
  parent.insertBefore(restored, parent.children.item(delta.childIndex));
  return { applied: true, selected: restored as SVGGraphicsElement };
}

/**
 * Moves one edit between the undo and redo stacks by applying it. A step that
 * no longer applies (its shape is gone) stays where it was rather than being
 * discarded, so the history keeps matching the document.
 */
export function stepEditHistory(
  root: SVGSVGElement,
  item: Item,
  forward: boolean,
): { applied: boolean; selected?: SVGGraphicsElement } {
  const source = forward ? item.redoStack : item.undoStack;
  const delta = source?.[source.length - 1];
  if (!source || !delta) return { applied: false };
  const result = applyDelta(root, delta, forward);
  if (!result.applied) return result;
  source.pop();
  const bytes = deltaBytes(delta);
  if (forward) {
    item.undoStack ||= [];
    item.undoStack.push(delta);
    item.redoBytes = Math.max(0, (item.redoBytes || 0) - bytes);
    item.undoBytes = (item.undoBytes || 0) + bytes;
  } else {
    item.redoStack ||= [];
    item.redoStack.push(delta);
    item.undoBytes = Math.max(0, (item.undoBytes || 0) - bytes);
    item.redoBytes = (item.redoBytes || 0) + bytes;
  }
  return result;
}

const editRevisions = new WeakMap<Item, number>();

/** Records that the item's live SVG changed (paint, delete, undo or redo). */
export function noteSvgEdit(item: Item) {
  editRevisions.set(item, (editRevisions.get(item) || 0) + 1);
}

/** The edit revision a save serializes; pass it to settleSavedSvg. */
export function beginSvgSave(item: Item) {
  return editRevisions.get(item) || 0;
}

/**
 * Applies a confirmed save. The item becomes clean, and its history is
 * cleared, only when no edit happened while the write was in flight;
 * otherwise those later edits stay unsaved, undoable and savable.
 */
export function settleSavedSvg(item: Item, savedSvg: string, revision: number) {
  item.saveError = false;
  if ((editRevisions.get(item) || 0) !== revision) return false;
  item.svgText = savedSvg;
  item.dirty = false;
  item.undoStack = [];
  item.redoStack = [];
  item.undoBytes = 0;
  item.redoBytes = 0;
  item.undoBaselineLost = false;
  item.editLimitReached = false;
  return true;
}
