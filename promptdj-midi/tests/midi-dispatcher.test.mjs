import { strict as assert } from 'node:assert';
import { test } from 'node:test';

const { MidiDispatcher } = await import('../utils/MidiDispatcher.ts');

function fakeInput(id) {
  return { id, name: `Device ${id}`, onmidimessage: null };
}

function installMidi(inputs) {
  const access = { inputs };
  Object.defineProperty(globalThis.navigator, 'requestMIDIAccess', {
    configurable: true,
    value: async () => access,
  });
}

function controlChange(input, cc, value) {
  input.onmidimessage?.({ data: new Uint8Array([0xb0, cc, value]) });
}

test('a device plugged in after MIDI was opened is selected and drives knobs after refresh', async () => {
  const inputs = new Map();
  installMidi(inputs);
  const dispatcher = new MidiDispatcher();
  const received = [];
  dispatcher.addEventListener('cc-message', (event) => received.push(event.detail));

  assert.deepEqual(await dispatcher.getMidiAccess(), []);
  assert.equal(dispatcher.activeMidiInputId, null);

  const keyboard = fakeInput('kbd');
  inputs.set('kbd', keyboard); // hot-plug, then the UI refreshes
  assert.deepEqual(await dispatcher.getMidiAccess(), ['kbd']);
  assert.equal(dispatcher.activeMidiInputId, 'kbd');

  controlChange(keyboard, 7, 99);
  assert.deepEqual(received, [{ cc: 7, value: 99, channel: 0 }]);
});

test('an unplugged active device falls back to a connected one on refresh', async () => {
  const first = fakeInput('a');
  const second = fakeInput('b');
  const inputs = new Map([['a', first], ['b', second]]);
  installMidi(inputs);
  const dispatcher = new MidiDispatcher();
  await dispatcher.getMidiAccess();
  assert.equal(dispatcher.activeMidiInputId, 'a');

  inputs.delete('a');
  await dispatcher.getMidiAccess();
  assert.equal(dispatcher.activeMidiInputId, 'b');
});

test('a user-chosen device stays active across refreshes', async () => {
  const inputs = new Map([['a', fakeInput('a')], ['b', fakeInput('b')]]);
  installMidi(inputs);
  const dispatcher = new MidiDispatcher();
  await dispatcher.getMidiAccess();
  dispatcher.activeMidiInputId = 'b';
  await dispatcher.getMidiAccess();
  assert.equal(dispatcher.activeMidiInputId, 'b');
});
