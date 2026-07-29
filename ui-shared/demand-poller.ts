export type DemandPollerOptions = {
  hasWork: () => boolean;
  poll: () => Promise<void>;
  present?: () => void;
  pollEveryMs: number;
  presentEveryMs?: number;
};

export class DemandPoller {
  private pollTimer: number | undefined;
  private presentTimer: number | undefined;
  private pollActive = false;
  private disposed = false;

  constructor(private readonly options: DemandPollerOptions) {}

  start() {
    if (this.disposed || !this.options.hasWork()) return;
    this.schedulePoll();
    this.schedulePresentation();
  }

  stop() {
    if (this.pollTimer !== undefined) window.clearTimeout(this.pollTimer);
    if (this.presentTimer !== undefined) window.clearTimeout(this.presentTimer);
    this.pollTimer = undefined;
    this.presentTimer = undefined;
  }

  dispose() {
    this.disposed = true;
    this.stop();
  }

  private schedulePoll() {
    if (
      this.disposed
      || this.pollActive
      || this.pollTimer !== undefined
      || !this.options.hasWork()
    ) return;
    this.pollTimer = window.setTimeout(() => {
      this.pollTimer = undefined;
      void this.runPoll();
    }, this.options.pollEveryMs);
  }

  private async runPoll() {
    if (this.disposed || !this.options.hasWork()) {
      this.stop();
      return;
    }
    this.pollActive = true;
    try {
      await this.options.poll();
    } finally {
      this.pollActive = false;
      if (this.options.hasWork()) this.schedulePoll();
      else this.stop();
    }
  }

  private schedulePresentation() {
    const { present, presentEveryMs } = this.options;
    if (
      !present
      || !presentEveryMs
      || this.disposed
      || this.presentTimer !== undefined
      || !this.options.hasWork()
    ) return;
    this.presentTimer = window.setTimeout(() => {
      this.presentTimer = undefined;
      if (!this.options.hasWork()) {
        this.stop();
        return;
      }
      present();
      this.schedulePresentation();
    }, presentEveryMs);
  }
}
