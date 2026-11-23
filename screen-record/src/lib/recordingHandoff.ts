/** Coordinates recording ownership with an application shutdown request. */
export class RecordingHandoff {
  private starting: Promise<void> | null = null;
  private resolveStart: (() => void) | null = null;
  private stopping: Promise<boolean> | null = null;
  private active = false;
  private preparing = false;

  beginStart(): boolean {
    if (this.preparing || this.starting || this.stopping || this.active) return false;
    this.starting = new Promise((resolve) => { this.resolveStart = resolve; });
    return true;
  }

  finishStart(started: boolean): void {
    this.active ||= started;
    const resolve = this.resolveStart;
    this.resolveStart = null;
    this.starting = null;
    resolve?.();
  }

  restorePending(): void {
    this.active = true;
  }

  beginRelease(): boolean {
    if (this.preparing || this.starting || this.stopping) return false;
    this.active = true;
    this.preparing = true;
    return true;
  }

  finishRelease(released: boolean): void {
    if (released) this.active = false;
    this.preparing = false;
  }

  stop(work: () => Promise<boolean>): Promise<boolean> {
    if (this.stopping) return this.stopping;
    if (!this.active) return Promise.resolve(true);
    const running = (async () => {
      try {
        const saved = await work();
        if (saved) this.active = false;
        return saved;
      } catch (error) {
        console.error("Failed to save the recording", error);
        return false;
      }
    })();
    this.stopping = running;
    void running.finally(() => { if (this.stopping === running) this.stopping = null; });
    return running;
  }

  async prepare(stop: () => Promise<boolean>): Promise<boolean> {
    if (this.preparing) return false;
    this.preparing = true;
    if (this.starting) await this.starting;
    const saved = this.stopping ? await this.stopping : this.active ? await stop() : true;
    if (!saved) this.preparing = false;
    return saved;
  }

  cancelPrepare(): void {
    this.preparing = false;
  }
}

export const recordingHandoff = new RecordingHandoff();

export async function waitForRecordingAck<T>(operation: Promise<T>, timeoutMs = 5_000): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      operation,
      new Promise<never>((_, reject) => {
        timer = setTimeout(() => reject(new Error("Recording save acknowledgement timed out")), timeoutMs);
      }),
    ]);
  } finally {
    if (timer !== undefined) clearTimeout(timer);
  }
}
