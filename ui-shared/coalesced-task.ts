/**
 * Runs an async task one at a time. A request made while it runs is not
 * dropped: it schedules exactly one more run after the current one, so state
 * that changed mid-run (a result published during a history read) is seen.
 * Every caller's promise settles once a run that started after its request ends.
 * The task should handle its own errors; a failure ends the pending reruns.
 */
export class CoalescedTask {
  private running?: Promise<void>;
  private rerun = false;
  private readonly task: () => Promise<void>;

  constructor(task: () => Promise<void>) {
    this.task = task;
  }

  run(): Promise<void> {
    if (this.running) {
      this.rerun = true;
      return this.running;
    }
    this.running = (async () => {
      try {
        do {
          this.rerun = false;
          await this.task();
        } while (this.rerun);
      } finally {
        // Only one run exists at a time, so this one owns the slot.
        this.rerun = false;
        this.running = undefined;
      }
    })();
    return this.running;
  }
}
