import { t } from "./i18n";
import "../../3d-generator-ui/src/input-dialog.css";
import "./input-validation.css";

export async function validateSvgInput(
  invoke: <T>(cmd: string, args?: unknown) => Promise<T>,
  path: string, name: string, choose: () => void,
): Promise<boolean> {
  for (;;) {
    let error = "validation_unavailable";
    try {
      const result = await invoke<{ error: string | null }>("validate_image", { path });
      if (result?.error === null) return true;
      if (result?.error === "image_too_large" || result?.error === "image_invalid") error = result.error;
    } catch { /* Retry is offered when the bridge cannot check the image. */ }
    const retry = error === "validation_unavailable";
    const action = await showDialog(name, retry, t(retry ? "imageCheckUnavailable"
      : error === "image_too_large" ? "imageTooLarge" : "imageInvalid"));
    if (action === "retry") continue;
    if (action === "choose") choose();
    return false;
  }
}

function showDialog(name: string, retry: boolean, message: string): Promise<string> {
  return new Promise((resolve) => {
    const previous = document.activeElement;
    const dialog = document.createElement("dialog");
    dialog.className = "input-validation-dialog";
    dialog.setAttribute("role", "alertdialog");
    dialog.setAttribute("aria-labelledby", "input-dialog-title");
    dialog.setAttribute("aria-describedby", "input-dialog-message");
    const title = document.createElement("h2");
    title.id = "input-dialog-title";
    title.textContent = t(retry ? "imageCheckTitle" : "imageChooseAnother");
    const filename = document.createElement("p");
    filename.className = "input-dialog-filename";
    filename.textContent = name;
    const detail = document.createElement("p");
    detail.id = "input-dialog-message";
    detail.textContent = message;
    const actions = document.createElement("div");
    actions.className = "dialog-actions";
    const close = document.createElement("button");
    close.textContent = t("close");
    const primary = document.createElement("button");
    primary.className = "input-dialog-primary";
    primary.textContent = t(retry ? "imageCheckRetry" : "imageChooseAnother");
    let result = "dismiss";
    close.addEventListener("click", () => dialog.close());
    primary.addEventListener("click", () => { result = retry ? "retry" : "choose"; dialog.close(); });
    dialog.addEventListener("close", () => {
      dialog.remove();
      if (previous instanceof HTMLElement && previous.isConnected) previous.focus();
      resolve(result);
    }, { once: true });
    actions.append(close, primary);
    dialog.append(title, filename, detail, actions);
    document.body.append(dialog);
    dialog.showModal();
    primary.focus();
  });
}
