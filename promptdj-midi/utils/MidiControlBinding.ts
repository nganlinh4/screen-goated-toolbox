/**
 * @license
 * SPDX-License-Identifier: Apache-2.0
*/
import type { ControlChange } from '../types';

type ControlChangeSource = Pick<EventTarget, 'addEventListener' | 'removeEventListener'>;

/**
 * One prompt control's subscription to the shared MIDI dispatcher. The
 * dispatcher outlives every control, so a control removed from the grid must
 * release its listener or it keeps reacting to (and reporting) MIDI knobs.
 */
export class MidiControlBinding {
  private readonly onChange: (change: ControlChange) => void;
  private release: (() => void) | null = null;

  constructor(onChange: (change: ControlChange) => void) {
    this.onChange = onChange;
  }

  connect(source: ControlChangeSource | null) {
    this.disconnect();
    if (!source) return;
    const listener = (event: Event) => this.onChange((event as CustomEvent<ControlChange>).detail);
    source.addEventListener('cc-message', listener);
    this.release = () => source.removeEventListener('cc-message', listener);
  }

  disconnect() {
    this.release?.();
    this.release = null;
  }
}
