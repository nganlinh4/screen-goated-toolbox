import type { InputValidationAction } from "./input-validation-flow.ts";

type DialogCopy = Readonly<{ name:string; title:string; message:string; primary:string; close:string }>;

export function showInputValidationDialog(
  copy: DialogCopy, primaryAction: "retry" | "choose", signal: AbortSignal,
): Promise<InputValidationAction> {
  return new Promise((resolve) => {
    if (signal.aborted) { resolve("dismiss"); return; }
    const previous = document.activeElement;
    const dialog = document.createElement("dialog");
    const identity = crypto.randomUUID();
    dialog.className = "input-validation-dialog";
    dialog.setAttribute("role","alertdialog");
    dialog.setAttribute("aria-labelledby","input-title-"+identity);
    dialog.setAttribute("aria-describedby","input-message-"+identity);
    const title = document.createElement("h2");
    title.id = "input-title-"+identity;
    title.textContent = copy.title;
    const filename = document.createElement("p");
    filename.className = "input-dialog-filename";
    filename.textContent = copy.name;
    const detail = document.createElement("p");
    detail.id = "input-message-"+identity;
    detail.textContent = copy.message;
    const actions = document.createElement("div");
    actions.className = "dialog-actions";
    const close = document.createElement("button");
    close.textContent = copy.close;
    const primary = document.createElement("button");
    primary.className = "input-dialog-primary";
    primary.textContent = copy.primary;
    let result: InputValidationAction = "dismiss";
    let settled = false;
    const finish = () => {
      if (settled) return;
      settled = true;
      signal.removeEventListener("abort",abort);
      dialog.remove();
      if (previous instanceof HTMLElement && previous.isConnected) previous.focus();
      resolve(signal.aborted ? "dismiss" : result);
    };
    const abort = () => { if (dialog.open) dialog.close(); finish(); };
    signal.addEventListener("abort",abort,{once:true});
    close.addEventListener("click",()=>dialog.close());
    primary.addEventListener("click",()=>{ result=primaryAction;dialog.close(); });
    dialog.addEventListener("cancel",event=>{ event.preventDefault();dialog.close(); });
    dialog.addEventListener("close",finish,{once:true});
    actions.append(close,primary);
    dialog.append(title,filename,detail,actions);
    document.body.append(dialog);
    try {
      if (signal.aborted) { finish(); return; }
      dialog.showModal();
      primary.focus();
    } catch { finish(); }
  });
}
