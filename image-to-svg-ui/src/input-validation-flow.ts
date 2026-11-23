export type InputValidationError = "image_too_large" | "image_invalid" | "validation_unavailable";
export type InputValidationAction = "dismiss" | "retry" | "choose";

/** The dialog's primary action: retry a failed check, otherwise pick another image. */
export function inputValidationPrimaryAction(error: InputValidationError): "retry" | "choose" {
  return error === "validation_unavailable" ? "retry" : "choose";
}

function waitWhileActive<T>(promise: Promise<T>, signal: AbortSignal): Promise<T | undefined> {
  if (signal.aborted) return Promise.resolve(undefined);
  return new Promise((resolve, reject) => {
    const cleanup = () => signal.removeEventListener("abort", aborted);
    const aborted = () => { cleanup(); resolve(undefined); };
    signal.addEventListener("abort", aborted, { once: true });
    promise.then((value) => { cleanup(); resolve(signal.aborted ? undefined : value); },
      (error) => { cleanup(); if (signal.aborted) resolve(undefined); else reject(error); });
  });
}

export async function runInputValidation(
  check: () => Promise<{ error: string | null }>,
  dialog: (error: InputValidationError, signal: AbortSignal) => Promise<InputValidationAction>,
  choose: () => void,
  signal: AbortSignal,
  current: () => boolean,
): Promise<boolean> {
  const active = () => !signal.aborted && current();
  while (active()) {
    let error: InputValidationError = "validation_unavailable";
    try {
      const result = await waitWhileActive(check(), signal);
      if (!active()) return false;
      if (result?.error === null) return true;
      if (result?.error === "image_too_large" || result?.error === "image_invalid") error = result.error;
    } catch {
      if (!active()) return false;
    }
    const action = await waitWhileActive(dialog(error, signal), signal);
    if (!active()) return false;
    if (action === "retry") continue;
    if (action === "choose") choose();
    return false;
  }
  return false;
}
