import type { AppState } from "./types";

export async function refreshCapabilities(
  state: AppState, invoke: <T>(cmd: string) => Promise<T>, update: () => void,
) {
  try {
    state.generationCapabilities = await invoke("generation_capabilities");
    update();
  } catch {
    state.generationCapabilities = { ready: false, optionalInstruction: { fast: false, quality: false } };
  }
}
