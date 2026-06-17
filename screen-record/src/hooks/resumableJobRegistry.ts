import { useEffect } from 'react';

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

/** Remember the active run for `namespace`, replacing any previous snapshot. */
export function saveResumableRun<TSnapshot>(namespace: string, snapshot: TSnapshot): void {
  RUN_REGISTRY.set(namespace, snapshot);
}

/** Forget the active run for `namespace` (call on every terminal state). */
export function clearResumableRun(namespace: string): void {
  RUN_REGISTRY.delete(namespace);
}

/** Read the cached run for `namespace`, if any. */
export function readResumableRun<TSnapshot>(namespace: string): TSnapshot | undefined {
  return RUN_REGISTRY.get(namespace) as TSnapshot | undefined;
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
): void {
  useEffect(() => {
    const cached = readResumableRun<TSnapshot>(namespace);
    if (!cached || cached.jobId === currentJobId) return;
    restore(cached);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [namespace, currentJobId]);
}
