const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { createRequire } = require('node:module');
const frontendRequire = createRequire(require.resolve('../../../../../screen-record/package.json'));
const { JSDOM } = frontendRequire('jsdom');

function harness(t) {
  const dom = new JSDOM('<div id="button-container"></div>', {runScripts: 'dangerously'});
  const { window } = dom;
  const timers = new Map(), microtasks = [], messages = [];
  let now = 0, nextTimer = 1, preview = false;
  window.performance.now = () => now;
  window.setTimeout = (callback, delay) => {
    const id = nextTimer++; timers.set(id, {callback, at: now + delay}); return id;
  };
  window.clearTimeout = id => timers.delete(id);
  window.queueMicrotask = callback => microtasks.push(callback);
  window.ipc = {postMessage: text => messages.push(JSON.parse(text))};
  window.__SGT_BUTTON_SCENE__ = {isGeometryPreviewActive: () => preview};
  const read = file => fs.readFileSync(path.join(__dirname, file), 'utf8');
  const script = read('runtime.rs').split('r#"')[1].split('"#')[0]
    .replace('#L10N_JSON#', '{}').replace('#ICON_SVGS_JSON#', '{}');
  window.eval(script);
  window.eval(read('source_anchor.js'));
  t.after(() => { dom.window.close(); timers.clear(); microtasks.length = 0; });
  function group(id, imageX = 0) {
    const element = window.document.createElement('div');
    element.className = 'button-group'; element.dataset.hwnd = String(id);
    element.innerHTML = '<input>';
    element.getBoundingClientRect = () => ({x: 800, y: 800, left: 800, right: 880,
      top: 800, bottom: 830, width: 80, height: 30});
    window.document.getElementById('button-container').appendChild(element);
    window.registeredWindows[id] = {sourceGeometry: {source: true,
      obstacles: [{x: imageX, y: 0, w: 100, h: 100}]}};
    return element;
  }
  function advance(milliseconds) {
    const end = now + milliseconds;
    let count = 0;
    while (true) {
      const next = [...timers.entries()].sort((a, b) => a[1].at - b[1].at)[0];
      if (!next || next[1].at > end) break;
      assert.ok(++count < 100, 'proximity expiry must not poll indefinitely');
      now = next[1].at; timers.delete(next[0]); next[1].callback();
    }
    now = end;
  }
  return {window, timers, messages, group, advance, jump: milliseconds => { now += milliseconds; },
    preview: value => { preview = value; },
    settle: () => { while (microtasks.length) microtasks.shift()(); }};
}

test('source reveal expires after leaving a long-stationary pointer without further cursor scripts', t => {
  const h = harness(t), group = h.group(7);
  h.window.updateCursorPosition(50, 50);
  assert.equal(group.style.opacity, '1');
  assert.equal(h.timers.size, 0);
  h.jump(60000);
  h.window.updateCursorPosition(500, 500);
  assert.equal(group.style.opacity, '1');
  assert.equal(h.timers.size, 1);
  h.advance(999);
  assert.equal(group.style.opacity, '1');
  h.advance(1);
  assert.equal(group.style.opacity, '0');
  assert.equal(h.timers.size, 0);
  assert.deepEqual(h.messages.at(-1).regions, []);
});

test('multiple source holds share one expiry and removing controls cancels its owned wakeup', t => {
  const h = harness(t), first = h.group(7), second = h.group(8, 200);
  h.window.updateCursorPosition(50, 50);
  h.window.updateCursorPosition(150, 150);
  h.advance(100);
  h.window.updateCursorPosition(250, 50);
  h.advance(100);
  h.window.updateCursorPosition(500, 500);
  assert.equal(h.timers.size, 1);
  h.advance(800);
  assert.equal(first.style.opacity, '0');
  assert.equal(second.style.opacity, '1');
  assert.equal(h.timers.size, 1);
  first.remove(); second.remove();
  h.window.updateButtonOpacity();
  assert.equal(h.timers.size, 0);
});

test('focus transitions recompute stationary source controls without a polling loop', t => {
  const h = harness(t), group = h.group(7), input = group.querySelector('input');
  h.window.updateCursorPosition(500, 500);
  assert.equal(group.style.opacity, '0');
  input.focus(); h.settle();
  assert.equal(group.style.opacity, '1');
  h.window.updateCursorPosition(50, 50);
  h.window.updateCursorPosition(500, 500);
  h.advance(1000);
  assert.equal(group.style.opacity, '1');
  assert.equal(h.timers.size, 0);
  input.blur(); h.settle();
  assert.equal(group.style.opacity, '0');
  assert.equal(h.timers.size, 0);
});

test('topology changes replace the expiry even when its absolute deadline is unchanged', t => {
  const h = harness(t), group = h.group(7);
  h.window.updateCursorPosition(50, 50);
  h.window.updateCursorPosition(500, 500);
  h.advance(400);
  h.window.__SGT_TOPOLOGY_EPOCH__ = '2';
  h.window.updateButtonOpacity();
  assert.equal(h.timers.size, 1);
  h.advance(600);
  assert.equal(group.style.opacity, '0');
  assert.equal(h.messages.at(-1).topology_epoch, '2');
  assert.equal(h.timers.size, 0);
});

test('stale topology and active drag expiry cannot republish cleared input regions', t => {
  const h = harness(t), group = h.group(7);
  h.window.updateCursorPosition(50, 50);
  h.window.updateCursorPosition(500, 500);
  const count = h.messages.length;
  h.window.__SGT_TOPOLOGY_EPOCH__ = '2';
  h.advance(1000);
  assert.equal(h.messages.length, count);
  h.window.updateButtonOpacity();
  h.window.updateCursorPosition(50, 50);
  h.window.updateCursorPosition(500, 500);
  const beforeDrag = h.messages.length;
  h.preview(true); h.advance(1000);
  assert.equal(h.messages.length, beforeDrag);
  assert.equal(h.timers.size, 0);
  h.preview(false); h.window.restoreButtonRegionsAfterDrag(4);
  assert.equal(group.style.opacity, '0');
  assert.equal(h.messages.at(-1).action, 'restore_clickable_regions');
  assert.equal(h.messages.at(-1).gesture_id, 4);
});

test('the scene drag owner blocks direct cursor and forced proximity publication until settlement', t => {
  const h = harness(t);
  h.group(7);
  h.window.updateCursorPosition(50, 50);
  h.window.updateCursorPosition(500, 500);
  h.window.__SGT_CONTROL_GEOMETRY_CACHE__ = () => ({clear() {}});
  h.window.updateWindows = () => {};
  h.window.applyHostCommand = () => {};
  h.window.eval(fs.readFileSync(path.join(__dirname, '../button_scene_runtime.js'), 'utf8'));
  h.window.applyHostCommand({type: 'external_drag', active: true});
  assert.equal(h.window.__SGT_BUTTON_SCENE__.isGeometryPreviewActive(), true);
  assert.deepEqual(h.messages.at(-1).regions, []);
  const count = h.messages.length;
  h.window.updateCursorPosition(60, 60);
  h.window.updateButtonOpacity(true);
  h.advance(1000);
  assert.equal(h.messages.length, count);
  h.window.applyHostCommand({type: 'drag_settled', gesture_id: null, cards: []});
  assert.equal(h.window.__SGT_BUTTON_SCENE__.isGeometryPreviewActive(), false);
  const settled = h.messages.length;
  h.window.updateCursorPosition(70, 70);
  assert.ok(h.messages.length > settled);
  assert.ok(h.messages.at(-1).regions.length > 0);
});
