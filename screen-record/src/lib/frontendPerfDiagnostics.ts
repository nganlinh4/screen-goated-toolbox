export interface FrontendPerfEvent {
  label: string;
  at: number;
  duration?: number;
}

export interface FrontendLongTaskEntry {
  name: string;
  startTime: number;
  duration: number;
}

export interface FrontendFrameProbeSummary {
  sampleCount: number;
  maxFrameDeltaMs: number;
  p95FrameDeltaMs: number;
}

export interface FrontendPerfSnapshot {
  events: FrontendPerfEvent[];
  longTasks: FrontendLongTaskEntry[];
  frameProbe: FrontendFrameProbeSummary | null;
  renderCounters: Record<string, number>;
}

type LongTaskObserver = PerformanceObserver & {
  observe(options: { entryTypes: string[] }): void;
};

const MAX_EVENTS = 300;
const MAX_LONG_TASKS = 300;
const state: {
  installed: boolean;
  observer: LongTaskObserver | null;
  events: FrontendPerfEvent[];
  longTasks: FrontendLongTaskEntry[];
  renderCounters: Record<string, number>;
  openSpans: Map<string, number>;
  frameDeltas: number[];
  frameProbeRunning: boolean;
  lastFrameAt: number | null;
  frameHandle: number | null;
} = {
  installed: false,
  observer: null,
  events: [],
  longTasks: [],
  renderCounters: {},
  openSpans: new Map(),
  frameDeltas: [],
  frameProbeRunning: false,
  lastFrameAt: null,
  frameHandle: null,
};

function nowMs() {
  return typeof performance !== "undefined" ? performance.now() : Date.now();
}

function percentile(values: number[], p: number) {
  if (values.length === 0) return 0;
  const sorted = [...values].sort((a, b) => a - b);
  const idx = Math.min(sorted.length - 1, Math.max(0, Math.ceil((p / 100) * sorted.length) - 1));
  return sorted[idx] ?? 0;
}

function nextFrame(timestamp: number) {
  if (!state.frameProbeRunning) return;
  if (state.lastFrameAt !== null) {
    state.frameDeltas.push(timestamp - state.lastFrameAt);
  }
  state.lastFrameAt = timestamp;
  state.frameHandle = window.requestAnimationFrame(nextFrame);
}

export function markFrontendPerfEvent(label: string) {
  state.events.push({ label, at: nowMs() });
  if (state.events.length > MAX_EVENTS) {
    state.events.splice(0, state.events.length - MAX_EVENTS);
  }
}

export function startFrontendPerfSpan(label: string) {
  state.openSpans.set(label, nowMs());
}

export function endFrontendPerfSpan(label: string) {
  const start = state.openSpans.get(label);
  if (start === undefined) return;
  state.openSpans.delete(label);
  state.events.push({ label, at: start, duration: nowMs() - start });
  if (state.events.length > MAX_EVENTS) {
    state.events.splice(0, state.events.length - MAX_EVENTS);
  }
}

export function countFrontendRender(label: string) {
  state.renderCounters[label] = (state.renderCounters[label] ?? 0) + 1;
}

export function startFrontendFrameProbe() {
  state.frameDeltas = [];
  state.lastFrameAt = null;
  state.frameProbeRunning = true;
  if (typeof window !== "undefined") {
    state.frameHandle = window.requestAnimationFrame(nextFrame);
  }
}

export function stopFrontendFrameProbe(): FrontendFrameProbeSummary {
  state.frameProbeRunning = false;
  if (state.frameHandle !== null) {
    window.cancelAnimationFrame(state.frameHandle);
    state.frameHandle = null;
  }
  return {
    sampleCount: state.frameDeltas.length,
    maxFrameDeltaMs: Math.max(0, ...state.frameDeltas),
    p95FrameDeltaMs: percentile(state.frameDeltas, 95),
  };
}

export function resetFrontendPerfDiagnostics() {
  state.events = [];
  state.longTasks = [];
  state.renderCounters = {};
  state.openSpans.clear();
  state.frameDeltas = [];
  state.lastFrameAt = null;
}

export function getFrontendPerfSnapshot(): FrontendPerfSnapshot {
  const frameProbe = state.frameDeltas.length > 0
    ? {
        sampleCount: state.frameDeltas.length,
        maxFrameDeltaMs: Math.max(0, ...state.frameDeltas),
        p95FrameDeltaMs: percentile(state.frameDeltas, 95),
      }
    : null;
  return {
    events: [...state.events],
    longTasks: [...state.longTasks],
    frameProbe,
    renderCounters: { ...state.renderCounters },
  };
}

export function installFrontendPerfDiagnostics() {
  if (
    state.installed ||
    typeof window === "undefined" ||
    typeof PerformanceObserver === "undefined" ||
    !PerformanceObserver.supportedEntryTypes?.includes("longtask")
  ) {
    state.installed = true;
    return;
  }

  const observer = new PerformanceObserver((list) => {
    for (const entry of list.getEntries()) {
      state.longTasks.push({
        name: entry.name || "",
        startTime: entry.startTime,
        duration: entry.duration,
      });
    }
    if (state.longTasks.length > MAX_LONG_TASKS) {
      state.longTasks.splice(0, state.longTasks.length - MAX_LONG_TASKS);
    }
  }) as LongTaskObserver;

  try {
    observer.observe({ entryTypes: ["longtask"] });
    state.observer = observer;
  } catch {
    state.observer = null;
  }
  state.installed = true;
}
