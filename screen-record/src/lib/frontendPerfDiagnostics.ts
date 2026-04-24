const RECENT_EVENT_LIMIT = 12;
const EVENT_RETENTION_MS = 2500;
const HEARTBEAT_INTERVAL_MS = 250;
const HEARTBEAT_STALL_MS = 80;
const LONG_TASK_MIN_MS = 80;
const PERF_LOG_DEDUPE_MS = 500;

type PerfEvent = {
  time: number;
  label: string;
};

const recentEvents: PerfEvent[] = [];
let installed = false;
let lastPerfLogAt = 0;

function compactRecentEvents(now: number) {
  while (recentEvents.length > 0 && now - recentEvents[0].time > EVENT_RETENTION_MS) {
    recentEvents.shift();
  }
  while (recentEvents.length > RECENT_EVENT_LIMIT) {
    recentEvents.shift();
  }
}

function recentEventSummary(now: number) {
  compactRecentEvents(now);
  if (recentEvents.length === 0) return 'none';
  return recentEvents
    .map((event) => `${Math.round(now - event.time)}ms:${event.label}`)
    .join(' | ');
}

export function markFrontendPerfEvent(label: string) {
  const now = performance.now();
  recentEvents.push({ time: now, label });
  compactRecentEvents(now);
}

function logFrontendStall(kind: string, durationMs: number, extra = '') {
  const now = performance.now();
  if (now - lastPerfLogAt < PERF_LOG_DEDUPE_MS) return;
  lastPerfLogAt = now;
  console.warn(
    `[FrontendPerf] ${kind} ms=${durationMs.toFixed(1)}${extra ? ` ${extra}` : ''} recent=${recentEventSummary(now)}`,
  );
}

export function installFrontendPerfDiagnostics() {
  if (installed || typeof window === 'undefined') return;
  installed = true;

  let expected = performance.now() + HEARTBEAT_INTERVAL_MS;
  window.setInterval(() => {
    const now = performance.now();
    const drift = now - expected;
    expected = now + HEARTBEAT_INTERVAL_MS;
    if (drift > HEARTBEAT_STALL_MS) {
      logFrontendStall('event-loop-stall', drift);
    }
  }, HEARTBEAT_INTERVAL_MS);

  const PerformanceObserverCtor = window.PerformanceObserver;
  if (!PerformanceObserverCtor) return;
  try {
    const observer = new PerformanceObserverCtor((list) => {
      for (const entry of list.getEntries()) {
        if (entry.duration >= LONG_TASK_MIN_MS) {
          logFrontendStall('long-task', entry.duration, `start=${entry.startTime.toFixed(1)}`);
        }
      }
    });
    observer.observe({ entryTypes: ['longtask'] });
  } catch {
    // Long Task API is not available in every WebView2 runtime.
  }
}
