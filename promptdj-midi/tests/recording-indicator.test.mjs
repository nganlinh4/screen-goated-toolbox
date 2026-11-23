import { strict as assert } from 'node:assert';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import './support/fake-live-music.mjs';

const { requestRecording } = await import('../utils/RecordingIndicator.ts');

function display() {
  const shown = [];
  return { shown, setRecording: active => shown.push(active) };
}

test('the record button does not show recording when no recorder started', () => {
  const button = display();
  assert.equal(requestRecording(() => false, button), false);
  assert.deepEqual(button.shown, [false]);
});

test('the record button shows recording once a recorder runs', () => {
  const button = display();
  assert.equal(requestRecording(() => true, button), true);
  assert.deepEqual(button.shown, [true]);
});

test('a recorder that throws while starting leaves the button idle', () => {
  const button = display();
  assert.throws(() => requestRecording(() => { throw new Error('no device'); }, button), /no device/);
  assert.deepEqual(button.shown, [false]);
});

test('the component no longer flips to recording before a recorder exists', () => {
  const component = readFileSync(new URL('../components/PromptDjMidi.ts', import.meta.url), 'utf8');
  const toggle = component.slice(component.indexOf('private toggleRecording()'));
  assert.doesNotMatch(toggle.slice(0, toggle.indexOf('\n  }\n')), /this\.isRecording = !this\.isRecording/);
  const entry = readFileSync(new URL('../index.tsx', import.meta.url), 'utf8');
  assert.match(entry, /requestRecording\(startRecording, pdjMidi\)/);
});
