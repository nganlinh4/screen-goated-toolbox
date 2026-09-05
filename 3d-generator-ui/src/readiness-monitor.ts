export function monitorReadiness(
  invoke: <T>(cmd: string) => Promise<T>, update: (state: string) => void,
) {
  let disposed = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let active = false;
  async function refresh() {
    if (disposed || active) return;
    active = true;
    try {
      const status = await invoke<string>("runtime_preparation_status");
      if (!disposed && ["ready", "preparing", "unavailable"].includes(status)) update(status);
    } catch { /* Preserve the last observed status during a bridge interruption. */ }
    finally {
      active = false;
      if (!disposed) timer = setTimeout(() => void refresh(), 2_000);
    }
  }
  void refresh();
  return () => { disposed = true; clearTimeout(timer); };
}
