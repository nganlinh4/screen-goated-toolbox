const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const test = require('node:test');
const assert = require('node:assert/strict');

test('passive text surfaces stay click-through across reveal and visibility updates', () => {
  const source = fs.readFileSync(path.join(__dirname, 'scene_runtime.js'), 'utf8');
  const code = source.slice(source.indexOf('function applyAppearance('),
    source.indexOf('function applyContentModel('));
  const node = () => ({ dataset: {}, style: { setProperty() {}, removeProperty() {} } });
  const entry = { card: node(), visualSurface: node(), directHost: node(), backdrop: node() };
  const context = vm.createContext({
    window: { devicePixelRatio: 1 }, entry,
    setSourceReplacementSurface() {}, syncSourceBackdrop() {}
  });
  vm.runInContext(code, context);
  for (const sourceReplacement of [true, false]) {
    for (const visible of [true, false, true]) {
      context.model = {
        visible, opacity: 100, source_replacement: sourceReplacement,
        controls: { inputPassthrough: true }
      };
      vm.runInContext('applyAppearance(entry, model)', context);
      assert.equal(entry.card.style.pointerEvents, 'none');
      assert.equal(entry.visualSurface.style.pointerEvents, 'none');
    }
  }
  context.model = { visible: true, opacity: 100, source_replacement: true, controls: {} };
  vm.runInContext('applyAppearance(entry, model)', context);
  assert.equal(entry.card.style.pointerEvents, 'auto');
  assert.equal(entry.visualSurface.style.pointerEvents, '');
});
