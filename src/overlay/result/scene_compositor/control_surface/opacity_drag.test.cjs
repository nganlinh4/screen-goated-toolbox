const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

function layoutContext() {
  const context = vm.createContext({
    window: { addEventListener() {} },
    document: { fonts: { addEventListener() {} } },
    ResizeObserver: class { observe() {} unobserve() {} },
    requestAnimationFrame() {},
    activeGrabbingSources: new Set(),
  });
  vm.runInContext(fs.readFileSync(path.join(__dirname, 'layout_runtime.js'), 'utf8'), context);
  return context;
}

function opacityGroup(value) {
  // Like an input element, the slider stores its value as a string.
  let current = String(value);
  const slider = { get value() { return current; }, set value(next) { current = String(next); } };
  const label = { textContent: `${value}%` };
  const group = {
    querySelector: selector => selector === '.opacity-slider-inline' ? slider
      : selector === '.opacity-value-inline' ? label : null,
  };
  return { group, slider, label };
}

test('host layout pushes do not move the opacity slider while it is dragged', () => {
  const context = layoutContext();
  const { group, slider, label } = opacityGroup(80);
  context.activeGrabbingSources.add('opacity');
  // An echo of an earlier step of the same drag.
  context.syncOpacityControl(group, 60);
  assert.equal(slider.value, '80');
  assert.equal(label.textContent, '80%');
  context.activeGrabbingSources.delete('opacity');
  context.syncOpacityControl(group, 75);
  assert.equal(slider.value, '75');
  assert.equal(label.textContent, '75%');
});
