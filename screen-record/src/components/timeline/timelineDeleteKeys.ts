/**
 * One routing rule for timeline Delete/Backspace keys:
 * - editable targets (text fields, contenteditable) always keep the key;
 * - a curve point under the pointer outranks a lane's range selection, so a
 *   stale selection in another lane never swallows the point delete.
 */
let hoveredPointClaims = 0;

export function isEditableKeyTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.tagName === "SELECT"
    || target.isContentEditable;
}

export function isDeleteKey(event: KeyboardEvent): boolean {
  return event.key === "Delete" || event.key === "Backspace";
}

/** Registers a hovered, deletable curve point; returns the release callback. */
export function claimHoveredPointDelete(): () => void {
  hoveredPointClaims += 1;
  let released = false;
  return () => {
    if (released) return;
    released = true;
    hoveredPointClaims = Math.max(0, hoveredPointClaims - 1);
  };
}

export function isHoveredPointDeleteClaimed(): boolean {
  return hoveredPointClaims > 0;
}
