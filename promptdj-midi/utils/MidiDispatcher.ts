/**
 * @license
 * SPDX-License-Identifier: Apache-2.0
*/
import type { ControlChange } from '../types';

/** Simple class for dispatching MIDI CC messages as events. */
export class MidiDispatcher extends EventTarget {
  private access: MIDIAccess | null = null;
  private denied: boolean = false;
  activeMidiInputId: string | null = null;

  /** Reset the MIDI state to allow retrying after denial */
  reset() {
    this.access = null;
    this.denied = false;
    this.activeMidiInputId = null;
  }

  async getMidiAccess(): Promise<string[]> {

    if (this.access) {
      return [...this.access.inputs.keys()];
    }

    if (!navigator.requestMIDIAccess) {
      throw new Error('Your browser does not support the Web MIDI API. For a list of compatible browsers, see https://caniuse.com/midi');
    }

    // If previously denied, give a more helpful message
    if (this.denied) {
      throw new Error('MIDI access was previously denied. Please restart the application to try again, or check that your MIDI device is connected.');
    }

    try {
      this.access = await navigator.requestMIDIAccess({ sysex: false });
      this.denied = false;
    } catch (e) {
      console.warn('MIDI Access refused or not available:', e);
      this.access = null;
      this.denied = true;
      throw new Error('MIDI access denied. Please connect a MIDI device and restart the application to try again.');
    }

    if (!this.access || !this.access.inputs) {
      this.denied = true;
      throw new Error('MIDI access unavailable. Please ensure a MIDI device is connected and restart the application.');
    }

    const inputIds = [...this.access.inputs.keys()];

    if (inputIds.length > 0 && this.activeMidiInputId === null) {
      this.activeMidiInputId = inputIds[0];
    }

    for (const input of this.access.inputs.values()) {
      input.onmidimessage = (event: MIDIMessageEvent) => {
        if (input.id !== this.activeMidiInputId) return;

        const { data } = event;
        if (!data) {
          console.error('MIDI message has no data');
          return;
        }

        const statusByte = data[0];
        const channel = statusByte & 0x0f;
        const messageType = statusByte & 0xf0;

        const isControlChange = messageType === 0xb0;
        if (!isControlChange) return;

        const detail: ControlChange = { cc: data[1], value: data[2], channel };
        this.dispatchEvent(
          new CustomEvent<ControlChange>('cc-message', { detail }),
        );
      };
    }

    return inputIds;
  }

  getDeviceName(id: string): string | null {
    if (!this.access) {
      return null;
    }
    const input = this.access.inputs.get(id);
    return input ? input.name : null;
  }
}
