const EDITABLE_SELECTOR =
  "input, textarea, select, [contenteditable='true'], [contenteditable='']";

export function guardPollRendering(root: HTMLElement, render: () => void): () => void {
  let composing = false;
  let deferred = false;

  const editing = () => {
    const active = root.ownerDocument.activeElement;
    return composing || (active instanceof HTMLElement && active.matches(EDITABLE_SELECTOR));
  };

  const flush = () => {
    if (!deferred || editing()) return;
    deferred = false;
    render();
  };

  root.addEventListener("compositionstart", () => {
    composing = true;
  });
  root.addEventListener("compositionend", () => {
    composing = false;
    window.setTimeout(flush, 0);
  });
  root.addEventListener("focusout", () => {
    window.setTimeout(flush, 0);
  });

  return () => {
    if (editing()) {
      deferred = true;
      return;
    }
    deferred = false;
    render();
  };
}
