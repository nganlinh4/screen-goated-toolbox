export type VisiblePreviewTarget = {
  key: string;
  element: HTMLElement;
};

type IdleWindow = Window & {
  requestIdleCallback?: (
    callback: () => void,
    options?: { timeout: number },
  ) => number;
  cancelIdleCallback?: (handle: number) => void;
};

export class VisiblePreviewScheduler {
  private readonly observer?: IntersectionObserver;
  private targets = new Map<string, HTMLElement>();
  private elementKeys = new WeakMap<Element, string>();
  private visible = new Set<string>();
  private forced = new Set<string>();
  private pending: string[] = [];
  private queued = new Set<string>();
  private runningKey = "";
  private interactionActive = false;
  private pauseUntil = 0;
  private idleHandle: number | undefined;
  private timerHandle: number | undefined;
  private resumeHandle: number | undefined;

  constructor(
    private readonly root: HTMLElement | null,
    private readonly load: (key: string, element: HTMLElement) => Promise<void>,
    rootMargin = "120px 0px",
  ) {
    if ("IntersectionObserver" in window) {
      this.observer = new IntersectionObserver(
        (entries) => {
          for (const entry of entries) {
            const key = this.elementKeys.get(entry.target);
            if (!key) continue;
            if (entry.isIntersecting) {
              this.visible.add(key);
              this.enqueue(key);
            } else {
              this.visible.delete(key);
            }
          }
        },
        { root, rootMargin, threshold: 0.01 },
      );
    }
  }

  bind(targets: VisiblePreviewTarget[], priorityKeys: string[] = []) {
    this.observer?.disconnect();
    this.targets = new Map();
    this.elementKeys = new WeakMap();
    this.visible.clear();
    this.forced = new Set(priorityKeys);

    for (const target of targets) {
      if (!target.key || target.element.dataset.previewReady === "true") continue;
      if (!this.targets.has(target.key)) this.targets.set(target.key, target.element);
      this.elementKeys.set(target.element, target.key);
      if (this.observer) {
        this.observer.observe(target.element);
      } else if (this.nearVisibleArea(target.element)) {
        this.visible.add(target.key);
        this.enqueue(target.key);
      }
    }

    this.pending = this.pending.filter((key) => this.targets.has(key));
    this.queued = new Set(this.pending);
    for (const key of priorityKeys.slice().reverse()) this.enqueue(key, true);
    this.schedule();
  }

  prioritize(key: string) {
    if (!key || !this.targets.has(key)) return;
    this.forced.add(key);
    this.enqueue(key, true);
    this.schedule();
  }

  setInteractionActive(active: boolean, settleMilliseconds = 160) {
    if (active) {
      this.interactionActive = true;
      this.clearResume();
      this.cancelScheduled();
      return;
    }
    this.interactionActive = false;
    this.pauseUntil = Math.max(this.pauseUntil, performance.now() + settleMilliseconds);
    this.scheduleResume();
  }

  hold(milliseconds: number) {
    this.pauseUntil = Math.max(this.pauseUntil, performance.now() + milliseconds);
    this.cancelScheduled();
    this.scheduleResume();
  }

  dispose() {
    this.observer?.disconnect();
    this.cancelScheduled();
    this.clearResume();
    this.targets.clear();
    this.visible.clear();
    this.forced.clear();
    this.pending = [];
    this.queued.clear();
  }

  private enqueue(key: string, priority = false) {
    if (!this.targets.has(key) || this.queued.has(key) || this.runningKey === key) return;
    if (priority) this.pending.unshift(key);
    else this.pending.push(key);
    this.queued.add(key);
    this.schedule();
  }

  private schedule() {
    if (this.isPaused()) {
      this.scheduleResume();
      return;
    }
    if (this.runningKey || this.idleHandle !== undefined
      || this.timerHandle !== undefined || !this.pending.length) return;
    const host = window as IdleWindow;
    if (host.requestIdleCallback) {
      this.idleHandle = host.requestIdleCallback(() => {
        this.idleHandle = undefined;
        void this.runOne();
      }, { timeout: 240 });
    } else {
      this.timerHandle = window.setTimeout(() => {
        this.timerHandle = undefined;
        void this.runOne();
      }, 16);
    }
  }

  private cancelScheduled() {
    const host = window as IdleWindow;
    if (this.idleHandle !== undefined) {
      host.cancelIdleCallback?.(this.idleHandle);
      this.idleHandle = undefined;
    }
    if (this.timerHandle !== undefined) {
      window.clearTimeout(this.timerHandle);
      this.timerHandle = undefined;
    }
  }

  private async runOne() {
    if (this.isPaused()) {
      this.scheduleResume();
      return;
    }
    let key = "";
    while (this.pending.length) {
      const candidate = this.pending.shift()!;
      this.queued.delete(candidate);
      if (this.targets.has(candidate)
        && (this.forced.has(candidate) || this.visible.has(candidate))) {
        key = candidate;
        break;
      }
    }
    if (!key) return;
    const element = this.targets.get(key);
    if (!element) return;

    this.runningKey = key;
    try {
      await this.load(key, element);
    } catch {
      // A preview is optional and may be retried after the next queue reconciliation.
    } finally {
      this.forced.delete(key);
      this.runningKey = "";
      this.schedule();
    }
  }

  private nearVisibleArea(element: HTMLElement) {
    const bounds = element.getBoundingClientRect();
    const rootBounds = this.root?.getBoundingClientRect()
      ?? { top: 0, right: innerWidth, bottom: innerHeight, left: 0 };
    return bounds.bottom >= rootBounds.top - 120
      && bounds.top <= rootBounds.bottom + 120
      && bounds.right >= rootBounds.left
      && bounds.left <= rootBounds.right;
  }

  private isPaused() {
    return this.interactionActive || performance.now() < this.pauseUntil;
  }

  private clearResume() {
    if (this.resumeHandle === undefined) return;
    window.clearTimeout(this.resumeHandle);
    this.resumeHandle = undefined;
  }

  private scheduleResume() {
    this.clearResume();
    if (this.interactionActive) return;
    const delay = Math.max(0, this.pauseUntil - performance.now());
    this.resumeHandle = window.setTimeout(() => {
      this.resumeHandle = undefined;
      if (this.isPaused()) this.scheduleResume();
      else this.schedule();
    }, delay);
  }
}
