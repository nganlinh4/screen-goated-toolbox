import { useEffect, useRef } from 'react';
import { invoke } from '@/lib/ipc';

/**
 * Shared polling primitive for the subtitle generation / translation /
 * narration / s2s hooks. It owns the duplicated control-flow scaffolding:
 *
 * - the polling effect lifecycle keyed on `jobId` (and an optional `restartKey`)
 * - the `cancelled` flag and the `if (cancelled) return` guards
 * - the `fetchStatus` call
 * - `onTick` per poll (runs after every successful fetch, including the poll
 *   that observes a terminal state — matching the original code, which always
 *   processed results before checking for the terminal branch)
 * - terminal detection via `isTerminal`
 * - dispatching `onComplete` (terminal status) / `onError` (thrown exception)
 * - rescheduling the next poll with a per-status delay from `intervalFor`
 * - cleanup on unmount / jobId change: sets `cancelled` and runs the optional
 *   `onCleanup` (used by hooks that register extra apply/status timers)
 *
 * CRITICAL fidelity properties (the original copies rely on these):
 * - `onTick`, `onComplete`, and `onError` may be ASYNC and are AWAITED, so a
 *   hook can `await` an apply-drain / finalize before clearing state.
 * - callbacks are read through a ref, so a callback identity change does NOT
 *   restart the loop or drop an in-flight poll; the loop only (re)starts when
 *   `jobId` / `enabled` / `restartKey` change. This mirrors the s2s hook's
 *   `latestRefs` pattern and is behaviorally equivalent to the other hooks'
 *   restart-on-callback-change (same job, same fresh logic).
 */
export interface AsyncJobPollConfig<Status> {
  /** The active job id, or null/undefined when no job is running. */
  jobId: string | null | undefined;
  /**
   * Extra gate beyond a truthy `jobId` (e.g. translation also requires a
   * `jobContext`). Defaults to `true`. When false, no polling runs.
   */
  enabled?: boolean;
  /**
   * Optional value that, when changed, restarts the polling loop in addition to
   * `jobId` (e.g. s2s's `backendMode`).
   */
  restartKey?: unknown;
  /** Fetch the latest status for `jobId`. */
  fetchStatus: (jobId: string) => Promise<Status>;
  /** Whether a status represents a terminal state (completed/cancelled/error). */
  isTerminal: (status: Status) => boolean;
  /** Per-poll processing. Runs after every successful fetch (terminal included). */
  onTick?: (status: Status) => void | Promise<void>;
  /** Terminal handler. Receives the terminal status; may branch on its state. */
  onComplete: (status: Status) => void | Promise<void>;
  /** Exception handler for a thrown fetch/processing error. */
  onError: (error: unknown) => void | Promise<void>;
  /** Delay (ms) before the next poll, for a non-terminal status. */
  intervalFor: (status: Status) => number;
  /** Optional extra cleanup (e.g. clearing apply/status timers) on teardown. */
  onCleanup?: () => void;
}

export function useAsyncJobPoll<Status>(config: AsyncJobPollConfig<Status>): void {
  const { jobId, enabled = true, restartKey } = config;

  // Always read the latest callbacks via a ref so callback identity changes do
  // not restart the loop or interrupt an in-flight poll.
  const configRef = useRef(config);
  configRef.current = config;

  useEffect(() => {
    if (!jobId || !enabled) return;
    let cancelled = false;

    const poll = async () => {
      const current = configRef.current;
      try {
        const status = await current.fetchStatus(jobId);
        if (cancelled) return;
        if (current.onTick) {
          await current.onTick(status);
        }
        if (current.isTerminal(status)) {
          await current.onComplete(status);
          return;
        }
        window.setTimeout(poll, current.intervalFor(status));
      } catch (error) {
        if (cancelled) return;
        await current.onError(error);
      }
    };

    void poll();
    return () => {
      cancelled = true;
      configRef.current.onCleanup?.();
    };
    // Restart only on these identity changes; everything else is read via ref.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [jobId, enabled, restartKey]);
}

export interface CancelHandlerConfig {
  /** The active job id, or null/undefined when there is nothing to cancel. */
  jobId: string | null | undefined;
  /** The IPC command that cancels the job (e.g. 'cancel_subtitle_generation'). */
  cancelCommand: string;
  /**
   * Hook-specific post-cancel work (status patch + state resets + any async
   * finalize). Runs after the cancel IPC resolves.
   */
  onCancelled?: () => void | Promise<void>;
}

/**
 * Shared skeleton for the hooks' cancel handlers: no-op when there is no job,
 * fire the cancel IPC, then run the hook-specific `onCancelled` work.
 */
export function buildCancelHandler(
  config: CancelHandlerConfig,
): () => Promise<void> {
  return async () => {
    const { jobId, cancelCommand, onCancelled } = config;
    if (!jobId) return;
    await invoke(cancelCommand, { jobId });
    await onCancelled?.();
  };
}
