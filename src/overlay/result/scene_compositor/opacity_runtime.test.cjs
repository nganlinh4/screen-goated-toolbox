const { test } = require('node:test');
const assert = require('node:assert/strict');
const { readFileSync } = require('node:fs');
const { join } = require('node:path');
const vm = require('node:vm');

test('opacity changes never reveal mounted hidden cards or alter content', () => {
  const hidden = { visible: false, card: { style: { opacity: '0' } }, body: '' };
  const visible = { visible: true, card: { style: { opacity: '1' } }, body: 'Complete text' };
  const cards = new Map([['1', hidden], ['2', visible]]);
  const context = vm.createContext({ cards, window: {
    updateWindows() {}, ipc: { postMessage() {} },
  }, document: {
    getElementById: () => ({ style: {} }),
    querySelector: () => { throw new Error('controls must not mutate result visuals'); },
  } });
  for (const script of ['host_command_runtime.js', 'button_scene_runtime.js']) {
    vm.runInContext(readFileSync(join(__dirname, script), 'utf8'), context);
  }
  for (const opacity of [100, 50, 0, 1, 100, 0, 100, -1, 101]) {
    for (const id of [1, 2, 999]) context.window.applyHostCommand({ type: 'opacity', id, opacity });
    assert.equal(hidden.card.style.opacity, '0');
    assert.equal(visible.card.style.opacity, String(Math.max(0, Math.min(100, opacity)) / 100));
    assert.equal(hidden.visible, false);
    assert.equal(visible.body, 'Complete text');
  }
  hidden.visible = true;
  context.window.applyHostCommand({ type: 'opacity', id: 1, opacity: 65 });
  assert.equal(hidden.card.style.opacity, '0.65');
  hidden.visible = false;
  context.window.applyHostCommand({ type: 'opacity', id: 1, opacity: 100 });
  assert.equal(hidden.card.style.opacity, '0');
});
