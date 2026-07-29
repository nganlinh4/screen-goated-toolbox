export type DestructiveConfirmationCopy = {
  message: string;
  confirmLabel: string;
  cancelLabel: string;
  cancelClass?: string;
};

export function confirmDestructive(copy: DestructiveConfirmationCopy): Promise<boolean> {
  return new Promise((resolve) => {
    const overlay = document.createElement("div");
    overlay.className = "app-dialog";
    overlay.setAttribute("role", "alertdialog");
    overlay.setAttribute("aria-modal", "true");
    const surface = document.createElement("div");
    surface.className = "dialog-surface";
    const message = document.createElement("strong");
    message.textContent = copy.message;
    const actions = document.createElement("div");
    actions.className = "dialog-actions";
    const cancel = document.createElement("button");
    cancel.type = "button";
    cancel.className = copy.cancelClass || "secondary";
    cancel.textContent = copy.cancelLabel;
    const accept = document.createElement("button");
    accept.type = "button";
    accept.className = "danger-action";
    accept.textContent = copy.confirmLabel;
    actions.append(cancel, accept);
    surface.append(message, actions);
    overlay.append(surface);
    document.body.append(overlay);

    const finish = (accepted: boolean) => {
      document.removeEventListener("keydown", onKey);
      overlay.remove();
      resolve(accepted);
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") finish(false);
    };
    cancel.addEventListener("click", () => finish(false), { once: true });
    accept.addEventListener("click", () => finish(true), { once: true });
    overlay.addEventListener("click", (event) => {
      if (event.target === overlay) finish(false);
    });
    document.addEventListener("keydown", onKey);
    accept.focus();
  });
}
