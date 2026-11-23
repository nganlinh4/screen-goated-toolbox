import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import './support/fake-live-music.mjs';

const { createPromptDjInitialState } = await import('../components/PromptDjMidi.state.ts');

const KEY = 'pdj_test_state';

function basePrompts(count = 24) {
  const prompts = new Map();
  for (let i = 0; i < count; i++) {
    prompts.set(`prompt-${i}`, { promptId: `prompt-${i}`, text: `Base ${i}`, weight: i === 3 ? 1 : 0, cc: i, color: '#fff' });
  }
  return prompts;
}

function withSaved(saved, run) {
  const store = new Map([[KEY, JSON.stringify(saved)]]);
  globalThis.localStorage = { getItem: key => store.get(key) ?? null, setItem: (key, value) => store.set(key, value) };
  try { return run(); } finally { delete globalThis.localStorage; }
}

test('a removed built-in prompt stays removed after reload instead of playing hidden', () => {
  const state = withSaved({ prompts: [], addSlotsActive: [false, false, false, false], removedSlots: ['prompt-3'] },
    () => createPromptDjInitialState(basePrompts(), KEY));
  assert.equal(state.removedSlots.has('prompt-3'), true);
  assert.equal(state.prompts.has('prompt-3'), false, 'a removed slot must not keep sending its prompt');
});

test('a built-in slot added beyond the initial grid survives reload', () => {
  const saved = { promptId: 'prompt-22', text: 'My groove', weight: 0.7, cc: 22, color: '#abc' };
  const state = withSaved({ prompts: [saved], addSlotsActive: [false, false, false, false], removedSlots: [] },
    () => createPromptDjInitialState(basePrompts(), KEY));
  assert.deepEqual(state.prompts.get('prompt-22'), saved);
  assert.equal(state.prompts.has('prompt-23'), false);
});
