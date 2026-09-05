export type InputError = "image_too_small" | "image_too_large" | "image_invalid";
type Invoke = <T = unknown>(cmd: string, args?: unknown) => Promise<T>;

export async function validateInput(
  invoke: Invoke, path: string, generationMode: "fast" | "quality",
): Promise<InputError | null> {
  try {
    const result = await invoke<{ error: InputError | null }>("validate_image", { path, generationMode });
    if (result?.error === null) return null;
    if (result?.error === "image_too_small" || result?.error === "image_too_large") return result.error;
  } catch {
    // An unavailable source or preflight must not start a remote job.
  }
  return "image_invalid";
}
