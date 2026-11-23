import { strict as assert } from 'node:assert';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import './support/fake-live-music.mjs';

const { MidiControlBinding } = await import('../utils/MidiControlBinding.ts');

const cc = (detail) => new CustomEvent('cc-message', { detail });

test('a disconnected prompt control stops reacting to MIDI', () => {
  const dispatcher = new EventTarget();
  const received = [];
  const binding = new MidiControlBinding(change => received.push(change.value));
  binding.connect(dispatcher);
  dispatcher.dispatchEvent(cc({ cc: 1, value: 10, channel: 0 }));
  binding.disconnect();
  dispatcher.dispatchEvent(cc({ cc: 1, value: 20, channel: 0 }));
  assert.deepEqual(received, [10]);
});

test('reconnecting keeps a single subscription', () => {
  const first = new EventTarget();
  const second = new EventTarget();
  const received = [];
  const binding = new MidiControlBinding(change => received.push(change.value));
  binding.connect(first);
  binding.connect(first);
  binding.connect(second);
  first.dispatchEvent(cc({ cc: 1, value: 1, channel: 0 }));
  second.dispatchEvent(cc({ cc: 1, value: 2, channel: 0 }));
  assert.deepEqual(received, [2]);
});

test('the prompt control releases its MIDI subscription when it leaves the page', () => {
  const source = readFileSync(new URL('../components/PromptController.ts', import.meta.url), 'utf8');
  assert.match(source, /disconnectedCallback\(\)\s*\{[^}]*midiBinding\.disconnect\(\)/);
});
