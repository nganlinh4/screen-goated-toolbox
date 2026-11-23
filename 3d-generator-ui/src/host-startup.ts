import { monitorReadiness } from "./readiness-monitor";
import { monitorRecovery } from "./recovery-monitor";
import type { AppState } from "./types";

type Invoke = <T = unknown>(command: string, args?: unknown) => Promise<T>;

export function startHostSession(options: {
  state: AppState;
  invoke: Invoke;
  updateUi: () => void;
  restoreCurrentJobs: () => Promise<boolean>;
  refreshHistory: () => Promise<void>;
  refreshCapabilities: () => Promise<void>;
}): void {
  const { state, invoke, updateUi, restoreCurrentJobs, refreshHistory, refreshCapabilities } = options;
  let disposed = false;
  window.addEventListener("pagehide", () => { disposed = true; }, { once: true });
  void (async () => {
    void invoke("prepare_runtime").catch(() => undefined);
    window.addEventListener("pagehide", monitorReadiness(invoke, (status) => {
      state.preparationStatus = status;
      updateUi();
    }), { once: true });
    try {
      state.outputDir = await invoke<string>("default_output_dir");
      updateUi();
    } catch {
      // The interface remains usable when the default directory is unavailable.
    }
    if (disposed) return;
    window.addEventListener("pagehide", monitorRecovery(
      () => invoke<boolean>("recovery_pending"),
      restoreCurrentJobs,
    ), { once: true });
    await refreshHistory();
    void refreshCapabilities();
    window.setTimeout(() => void refreshCapabilities(), 1_000);
  })();
}
