const EDITABLE_SELECTOR =
  "input, textarea, select, [contenteditable='true'], [contenteditable='']";

export function guardPollRendering(root: HTMLElement, render: () => void): () => void {
  let composing = false;
  let deferred = false;
  let pointerActive = false;

  const protectedInteraction = () => {
    const active = root.ownerDocument.activeElement;
    return pointerActive
      || composing
      || (active instanceof HTMLElement && active.matches(EDITABLE_SELECTOR))
      || Boolean(root.querySelector(".queue-rail:hover"));
  };

  const flush = () => {
    if (!deferred || protectedInteraction()) return;
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
  root.addEventListener("pointerout", () => {
    window.setTimeout(flush, 0);
  });
  root.addEventListener("pointerdown", () => {
    pointerActive = true;
  }, true);
  root.addEventListener("pointerup", () => {
    window.setTimeout(() => {
      pointerActive = false;
      flush();
    }, 0);
  }, true);
  root.addEventListener("pointercancel", () => {
    pointerActive = false;
    window.setTimeout(flush, 0);
  }, true);

  return () => {
    if (protectedInteraction()) {
      deferred = true;
      return;
    }
    deferred = false;
    render();
  };
}
