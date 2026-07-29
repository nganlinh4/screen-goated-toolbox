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
