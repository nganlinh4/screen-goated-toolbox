export class LatestOnlyLane<T> {
  private revision = 0;
  private activeAbort?: AbortController;
  private settled: Promise<void> = Promise.resolve();

  invalidate() {
    this.revision += 1;
    this.activeAbort?.abort();
  }

  async run(
    load: (signal: AbortSignal) => Promise<T>,
    dispose: (value: T) => void,
  ): Promise<T | null> {
    const revision = ++this.revision;
    this.activeAbort?.abort();
    const abort = new AbortController();
    this.activeAbort = abort;
    const predecessor = this.settled;
    await predecessor;
    if (revision !== this.revision || abort.signal.aborted) return null;

    const operation = load(abort.signal);
    this.settled = operation.then(
      () => undefined,
      () => undefined,
    );
    try {
      const value = await operation;
      if (revision !== this.revision || abort.signal.aborted) {
        dispose(value);
        return null;
      }
      return value;
    } catch (error) {
      if (revision !== this.revision || abort.signal.aborted) return null;
      throw error;
    } finally {
      if (this.activeAbort === abort) this.activeAbort = undefined;
    }
  }
}
