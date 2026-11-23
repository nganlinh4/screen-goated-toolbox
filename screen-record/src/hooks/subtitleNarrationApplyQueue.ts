import type { SubtitleNarrationJobStatus, SubtitleNarrationResultItem } from './subtitleNarrationHelpers';

type DrainWaiter = { resolve: () => void; reject: (error: unknown) => void };

/** Shared by panel instances so queued and in-flight work survives a remount. */
export interface SubtitleNarrationApplyQueue {
  targets?: Map<string, import('./subtitleNarrationHelpers').SubtitleNarrationRequestItem>;
  pending: SubtitleNarrationResultItem[];
  appliedIds: Set<string>;
  timer: number | null;
  applying: boolean;
  failure: unknown | null;
  waiters: DrainWaiter[];
  completion: Promise<void> | null;
  retired: boolean;
  terminalStatus: SubtitleNarrationJobStatus | null;
  listeners: Set<() => void>;
}

export function createSubtitleNarrationApplyQueue(): SubtitleNarrationApplyQueue {
  return { pending: [], appliedIds: new Set(), timer: null, applying: false, failure: null, waiters: [], completion: null, retired: false, terminalStatus: null, listeners: new Set() };
}

export function notifySubtitleNarrationQueue(queue: SubtitleNarrationApplyQueue): void {
  queue.listeners.forEach((listener) => listener());
}

export function settleSubtitleNarrationDrain(queue: SubtitleNarrationApplyQueue): void {
  if (!queue.failure && (queue.pending.length || queue.applying)) return;
  const waiters = queue.waiters.splice(0);
  for (const waiter of waiters) {
    if (queue.failure) waiter.reject(queue.failure);
    else waiter.resolve();
  }
}
