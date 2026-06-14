export type AsyncJobState =
  | 'queued'
  | 'running'
  | 'completed'
  | 'cancelled'
  | 'error';

/**
 * Common fields shared by the async job status objects used across subtitle
 * generation, translation, and narration hooks.
 *
 * Note: the optional `error`/`messageKey`/`messageParams` fields are declared
 * with the exact (nullable) types the existing status interfaces use, so those
 * interfaces can extend this base without altering their public shape.
 */
export interface BaseAsyncJobStatus {
  state: AsyncJobState;
  message: string;
  progress: number;
  error?: string | null;
  messageKey?: string | null;
  messageParams?: Record<string, string> | null;
}
