/**
 * @license
 * SPDX-License-Identifier: Apache-2.0
*/

/** The part of the UI that shows whether a recording is running. */
export interface RecordingDisplay {
  setRecording(active: boolean): void;
}

/**
 * Starts a recording and shows the outcome. The record button follows the
 * recorder rather than the click: a click can fail to start one (no API key
 * yet, no audio graph, a recorder error), and the recorder's own stop clears
 * the button through the same display.
 */
export function requestRecording(start: () => boolean, display: RecordingDisplay): boolean {
  let started = false;
  try {
    started = start();
  } finally {
    display.setRecording(started);
  }
  return started;
}
