/**
 * @license
 * SPDX-License-Identifier: Apache-2.0
*/

/** The parts of a live music helper that settings replacement must release. */
export interface RetirableHelper {
  dispose(): void;
  readonly audioContext: { close(): Promise<void> };
}

/** Settings refreshes resend the same key; only a new key needs a new helper. */
export function needsNewHelper(currentKey: string | null, nextKey: string, hasHelper: boolean) {
  return !hasHelper || currentKey !== nextKey;
}

/**
 * Releases a helper that is being replaced. A helper that still owns an active
 * recording keeps its context open; the recorder closes it after encoding.
 */
export function retireHelper<T extends RetirableHelper>(
  previous: T,
  recordingOwner: T | null,
  stopRecording: () => void,
  analyser: { stop(): void } | null,
) {
  const recording = recordingOwner === previous;
  if (recording) stopRecording();
  analyser?.stop();
  previous.dispose();
  if (!recording) void previous.audioContext.close();
}
