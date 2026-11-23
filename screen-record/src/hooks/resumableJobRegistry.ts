import { useCallback, useEffect, useSyncExternalStore } from 'react';

/**
 * Keeps a still-running backend generation job "alive" across panel unmounts.
 *
 * The generation hooks (subtitle generation / translation / narration / s2s)
 * keep their `jobId` and polling state in component-local React state. When the
 * user switches side-panel tabs the panel unmounts, that state is lost, and the
 * UI looks idle again even though the backend job is still running — pressing
 * generate again then hits "… already running". `useAsyncJobPoll`'s cleanup only
 * stops the poll loop; it never cancels the backend job, so the job can always
 * be re-adopted once we remember its id.
 *
 * This module is a tiny module-level cache (it lives for the lifetime of the
 * WebView, independent of React's tree) keyed by a stable per-feature namespace.
 * Each hook stores whatever snapshot it needs to resume — at minimum the
 * `jobId` — and rehydrates from it on mount via {@link useResumableRun}.
 */
const RUN_REGISTRY = new Map<string, unknown>();
const LISTENERS = new Map<string, Set<() => void>>();
const VERSIONS = new Map<string, number>();
const STARTS = new Map<string, { token: symbol; metadata: unknown; cancellation?: { command: string; jobId: string } }>();

function changed(namespace: string): void {
  if (!LISTENERS.has(namespace) && !RUN_REGISTRY.has(namespace) && !STARTS.has(namespace)) {
    VERSIONS.delete(namespace);
    return;
  }
  VERSIONS.set(namespace, (VERSIONS.get(namespace) ?? 0) + 1);
  LISTENERS.get(namespace)?.forEach((listener) => listener());
}

function subscribe(namespace: string, listener: () => void): () => void {
  const listeners = LISTENERS.get(namespace) ?? new Set<() => void>();
  listeners.add(listener);
  LISTENERS.set(namespace, listeners);
  return () => {
    listeners.delete(listener);
    if (!listeners.size) {
      LISTENERS.delete(namespace);
      if (!RUN_REGISTRY.has(namespace) && !STARTS.has(namespace)) VERSIONS.delete(namespace);
    }
  };
}

/** Claim the asynchronous start before sending IPC so remounts cannot start twice. */
export function claimResumableStart(namespace: string, metadata?: unknown): symbol | null {
  if (STARTS.has(namespace) || RUN_REGISTRY.has(namespace)) return null;
  const token = Symbol(namespace);
  STARTS.set(namespace, { token, metadata });
  changed(namespace);
  return token;
}

export function releaseResumableStart(namespace: string, token: symbol): void {
  if (STARTS.get(namespace)?.token !== token) return;
  STARTS.delete(namespace);
  changed(namespace);
}

export function attachResumableStartCancellation(namespace: string, token: symbol, command: string, jobId: string): void {
  const start = STARTS.get(namespace);
  if (start?.token === token) start.cancellation = { command, jobId };
}

export function takeResumableStarts(predicate: (namespace: string) => boolean): Array<{ command: string; jobId: string }> {
  const cancellations: Array<{ command: string; jobId: string }> = [];
  for (const [namespace, start] of STARTS) {
    if (!predicate(namespace)) continue;
    STARTS.delete(namespace);
    changed(namespace);
    if (start.cancellation) cancellations.push(start.cancellation);
  }
  return cancellations;
}

export function readResumableStartMetadata<T>(namespace: string): T | undefined {
  return STARTS.get(namespace)?.metadata as T | undefined;
}

export function hasResumableStart(namespace: string): boolean {
  return STARTS.has(namespace);
}

export function ownsResumableStart(namespace: string, token: symbol): boolean {
  return STARTS.get(namespace)?.token === token;
}

export function useResumableStartPending(namespace: string): boolean {
  const subscribeForNamespace = useCallback((listener: () => void) => subscribe(namespace, listener), [namespace]);
  const getSnapshot = useCallback(() => STARTS.has(namespace), [namespace]);
  return useSyncExternalStore(subscribeForNamespace, getSnapshot, () => false);
}

/** Remember the active run for `namespace`, replacing any previous snapshot. */
export function saveResumableRun<TSnapshot>(namespace: string, snapshot: TSnapshot): void {
  RUN_REGISTRY.set(namespace, snapshot);
  changed(namespace);
}

/** Forget the active run for `namespace` (call on every terminal state). */
export function clearResumableRun(namespace: string): void {
  if (RUN_REGISTRY.delete(namespace)) changed(namespace);
}

/** Remove only the run that produced an asynchronous completion. */
export function clearResumableRunIfJob(namespace: string, jobId: string): boolean {
  const current = RUN_REGISTRY.get(namespace) as { jobId?: unknown } | undefined;
  if (current?.jobId !== jobId) return false;
  clearResumableRun(namespace);
  return true;
}

/** Read the cached run for `namespace`, if any. */
export function readResumableRun<TSnapshot>(namespace: string): TSnapshot | undefined {
  return RUN_REGISTRY.get(namespace) as TSnapshot | undefined;
}

/** Remove and return runs whose namespace belongs to a retired owner. */
export function takeResumableRuns(
  predicate: (namespace: string) => boolean,
): Array<[string, unknown]> {
  const removed: Array<[string, unknown]> = [];
  for (const [namespace, snapshot] of RUN_REGISTRY) {
    if (!predicate(namespace)) continue;
    RUN_REGISTRY.delete(namespace);
    changed(namespace);
    removed.push([namespace, snapshot]);
  }
  return removed;
}

/**
 * On mount (and whenever the tracked job changes), re-adopt a cached run for
 * `namespace` that isn't the one we're already tracking, so a remounted panel
 * resumes the in-flight job instead of looking idle. `restore` should rehydrate
 * the hook's state (set `jobId`, status, any cursors) and let polling resume.
 *
 * `restore` is intentionally excluded from the effect dependencies: it is a
 * fresh closure each render, and re-running on identity change would re-restore
 * a job we already adopted. Setting `jobId` inside `restore` re-runs the effect
 * once, which then short-circuits because the cached job now matches.
 */
export function useResumableRun<TSnapshot extends { jobId: string }>(
  namespace: string,
  currentJobId: string | null,
  restore: (snapshot: TSnapshot) => void,
  retired?: (jobId: string) => void,
  ownerKey?: unknown,
): void {
  const subscribeForNamespace = useCallback((listener: () => void) => subscribe(namespace, listener), [namespace]);
  const getSnapshot = useCallback(() => VERSIONS.get(namespace) ?? 0, [namespace]);
  const version = useSyncExternalStore(subscribeForNamespace, getSnapshot, () => 0);
  useEffect(() => {
    const cached = readResumableRun<TSnapshot>(namespace);
    if (!cached) {
      if (currentJobId) retired?.(currentJobId);
      return;
    }
    if (cached.jobId === currentJobId) return;
    restore(cached);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [namespace, currentJobId, version, ownerKey]);
}
