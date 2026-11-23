// Card edge resizing: live typography preview, the debounced smart fit and the
// final fit, driven through real pointer events on the attached handles.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const read = name => fs.readFileSync(path.join(__dirname, name), 'utf8');

function element() {
  return {
    dataset: {}, children: [], listeners: {}, style: {},
    appendChild(child) { this.children.push(child); },
    remove() {},
    addEventListener(name, callback) { (this.listeners[name] ||= []).push(callback); },
  };
}

function harness({ externalNavigation = false } = {}) {
  const log = [];
  const frames = [];
  const timers = [];
  const documentListeners = {};
  const layoutReads = { count: 0 };
  const body = {
    textContent: 'a few words of result text', style: { fontSize: '20px', lineHeight: '30px' },
    get scrollHeight() { layoutReads.count++; return 60; },
    get offsetHeight() { layoutReads.count++; return 60; },
    getBoundingClientRect() { layoutReads.count++; return { width: 200, height: 100 }; },
  };
  const card = element();
  card.dataset.id = '7';
  card.style.transform = 'translate3d(10px,20px,0)';
  card.getBoundingClientRect = () => ({ left: 10, top: 20, width: 200, height: 100 });
  const fitState = {};
  const entry = {
    card, mode: 'direct', externalNavigation,
    processing: { resize() {} },
    requestResizeFit: () => log.push(['fit', body.style.fontSize]),
    directRuntime: { beginResizePreview: size =>
      window.__SGT_TYPOGRAPHY_RESIZE__.begin(body, fitState, size, () => {}) },
  };
  const window = {
    devicePixelRatio: 1,
    ipc: { postMessage: raw => { const m = JSON.parse(raw); log.push([m.action, body.style.fontSize, m]); } },
    addEventListener() {},
    __SGT_BUTTON_SCENE__: { allocateGestureId: () => 3, setDragActive: (active, id) => log.push(['drag', active, id]) },
  };
  const context = vm.createContext({
    window, getComputedStyle: () => ({}),
    document: { createElement: element,
      addEventListener(name, callback) { (documentListeners[name] ||= []).push(callback); } },
    requestAnimationFrame: callback => frames.push(callback) && frames.length,
    cancelAnimationFrame() {},
    setTimeout: (callback, delay) => { timers.push({ callback, delay }); return timers.length; },
    clearTimeout: id => { if (timers[id - 1]) timers[id - 1].cleared = true; },
  });
  vm.runInContext(read('resize_typography_runtime.js'), context);
  vm.runInContext(read('resize_runtime.js').replaceAll('__SGT_MIN_WINDOW_WIDTH_PX__', '100')
    .replaceAll('__SGT_MIN_WINDOW_HEIGHT_PX__', '60'), context);
  const attached = window.__SGT_CARD_RESIZE__.attach(entry);
  const pointer = (x, y, type = 'pointermove') => ({ type, pointerId: 1, button: 0, clientX: x, clientY: y,
    pointerType: 'mouse', preventDefault() {}, stopPropagation() {}, currentTarget: { setPointerCapture() {} } });
  return {
    log, frames, timers, body, card, layoutReads, window, destroy: () => attached.destroy(),
    start(edge = 'se') {
      card.children.find(handle => handle.dataset.edge === edge).listeners.pointerdown[0](pointer(0, 0, 'pointerdown'));
    },
    move(x, y) { documentListeners.pointermove[0](pointer(x, y)); },
    frame() { frames.splice(0).forEach(callback => callback(16)); },
    release(x, y) { documentListeners.pointerup[0](pointer(x, y, 'pointerup')); },
    fireTimers() { timers.filter(timer => !timer.cleared && !timer.fired).forEach(timer => { timer.fired = true; timer.callback(); }); },
  };
}

test('each preview frame resizes the card and its typography before the native preview', () => {
  const h = harness({ externalNavigation: true });
  h.start();
  assert.equal(h.log[0][0], 'result_resize_start');
  assert.deepEqual(h.log[1], ['drag', true, 3]);
  h.move(200, 100);
  h.frame();
  assert.equal(h.card.style.width, '400px');
  assert.equal(h.card.style.height, '200px');
  const preview = h.log.find(entry => entry[0] === 'result_resize_preview');
  assert.ok(parseFloat(preview[1]) > 20, 'typography grows before the preview is published');
  assert.deepEqual([preview[2].dx, preview[2].dy, preview[2].gesture_id], [200, 100, 3]);
});

test('the typography preview never reads layout while the size changes', () => {
  const h = harness();
  h.start();
  const afterBegin = h.layoutReads.count;
  for (const [x, y] of [[50, 20], [120, 60], [-40, -20]]) { h.move(x, y); h.frame(); }
  assert.equal(h.layoutReads.count, afterBegin);
  h.body.style.fontSize = '11px';
  h.move(30, 30); h.frame();
  assert.equal(h.layoutReads.count, afterBegin + 1, 'an external refit rebases once');
});

test('a stable size asks for one smart fit after 100 ms, and release does not repeat it', () => {
  const h = harness();
  h.start();
  h.move(100, 0); h.frame();
  h.move(120, 0); h.frame();
  const pending = h.timers.filter(timer => !timer.cleared);
  assert.equal(pending.length, 1);
  assert.equal(pending[0].delay, 100);
  h.fireTimers();
  assert.equal(h.log.filter(entry => entry[0] === 'fit').length, 1);
  h.release(120, 0);
  assert.equal(h.log.filter(entry => entry[0] === 'fit').length, 1);
  assert.equal(h.log.at(-1)[0], 'result_resize_finish');
  assert.equal(h.log.at(-1)[2].cancelled, false);
});

test('release renders the final size and requests its fit before reporting the finish', () => {
  const h = harness();
  h.start();
  h.move(100, 0); h.frame();
  h.release(150, 40);
  assert.equal(h.card.style.width, '350px');
  const order = h.log.map(entry => entry[0]);
  assert.ok(order.lastIndexOf('fit') < order.indexOf('result_resize_finish'));
  assert.equal(h.log.at(-1)[2].dx, 150);
});

test('west and north edges keep the opposite edge fixed and respect the minimum size', () => {
  const h = harness();
  h.start('nw');
  h.move(150, 80); h.frame();
  assert.equal(h.card.style.width, '100px');
  assert.equal(h.card.style.height, '60px');
  assert.equal(h.card.style.transform, 'translate3d(110px,60px,0)');
});

test('destroying a card mid-resize ends the gesture so controls can return', () => {
  const h = harness();
  h.start();
  h.move(40, 40);
  h.log.length = 0;
  h.destroy();
  assert.deepEqual(h.log, [['drag', false, 3]]);
  h.release(80, 80);
  assert.equal(h.log.some(entry => entry[0] === 'result_resize_finish'), false, 'a destroyed gesture never finishes');
});
