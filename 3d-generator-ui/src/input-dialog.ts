import { t } from "./i18n";
import type { InputError } from "./input-validation";
import "./input-dialog.css";

type Action = "dismiss" | "choose" | "retry";
let tail: Promise<unknown> = Promise.resolve();

export function showInputDialog(error: InputError, name: string): Promise<Action> {
  const next = tail.then(() => openDialog(error, name));
  tail = next.catch(() => undefined);
  return next;
}

function openDialog(error: InputError, name: string): Promise<Action> {
  return new Promise((resolve) => {
    const previous = document.activeElement;
    const dialog = document.createElement("dialog");
    dialog.className = "input-validation-dialog";
    dialog.setAttribute("role", "alertdialog");
    dialog.setAttribute("aria-labelledby", "input-dialog-title");
    dialog.setAttribute("aria-describedby", "input-dialog-message");
    const title = document.createElement("h2");
    title.id = "input-dialog-title";
    title.textContent = t(error === "validation_unavailable" ? "imageCheckTitle" : "imageDialogTitle");
    const filename = document.createElement("p");
    filename.className = "input-dialog-filename";
    filename.textContent = name;
    const message = document.createElement("p");
    message.id = "input-dialog-message";
    const keys = { image_too_small: "imageTooSmall", image_too_large: "imageTooLarge",
      image_invalid: "imageInvalid", validation_unavailable: "imageCheckUnavailable" } as const;
    message.textContent = t(keys[error]);
    const actions = document.createElement("div");
    actions.className = "dialog-actions";
    const cancel = document.createElement("button");
    cancel.textContent = t("close");
    const primary = document.createElement("button");
    primary.className = "input-dialog-primary";
    primary.textContent = t(error === "validation_unavailable" ? "imageCheckRetry" : "imageChooseAnother");
    let action: Action = "dismiss";
    cancel.addEventListener("click", () => dialog.close());
    primary.addEventListener("click", () => {
      action = error === "validation_unavailable" ? "retry" : "choose";
      dialog.close();
    });
    dialog.addEventListener("close", () => {
      dialog.remove();
      if (previous instanceof HTMLElement && previous.isConnected) previous.focus();
      resolve(action);
    }, { once: true });
    actions.append(cancel, primary);
    dialog.append(title, filename, message, actions);
    document.body.append(dialog);
    dialog.showModal();
    primary.focus();
  });
}
