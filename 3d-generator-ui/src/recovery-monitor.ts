type Timer = ReturnType<typeof globalThis.setTimeout>;

export function monitorRecovery(
  pending: () => Promise<boolean>,
  discover: () => Promise<boolean>,
  timers = { set: globalThis.setTimeout, clear: globalThis.clearTimeout },
): () => void {
  let stopped = false;
  let timer: Timer | undefined;
  async function poll() {
    let retry = true;
    try {
      const isPending = await pending();
      if (stopped) return;
      const discovered = await discover();
      retry = isPending || !discovered;
    } catch {
      // A failed bridge call is retried without overlapping another poll.
    }
    if (!stopped && retry) timer = timers.set(() => void poll(), 2_000);
  }
  void poll();
  return () => {
    stopped = true;
    if (timer !== undefined) timers.clear(timer);
  };
}
