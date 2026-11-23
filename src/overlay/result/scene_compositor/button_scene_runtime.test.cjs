// The control scene that mirrors result cards: batching, drag hiding and the
// settlement handshake that hands a drag preview back to committed geometry.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

function harness() {
  const log = [];
  const layouts = [];
  const container = { style: { visibility: '' } };
  const window = {
    devicePixelRatio: 1,
    __SGT_CONTROL_GEOMETRY_CACHE__: () => ({ merge() {}, clear() {}, get: () => null }),
    ipc: { postMessage: raw => log.push(['ipc', JSON.parse(raw)]) },
    invalidateButtonRegions: () => log.push(['invalidate']),
    updateWindows: windows => { layouts.push(windows); log.push(['layout', Object.keys(windows)]); },
    restoreButtonRegionsAfterDrag: id => log.push(['restore', id]),
    updateCursorPosition: (x, y) => log.push(['cursor', x, y]),
    clearResultDragControlPreview: () => log.push(['clear-preview']),
    releaseResultDragGeometryLock: () => log.push(['unlock']),
    setWindowButtonStackOrder() {},
    applyHostCommand: command => log.push(['result', command.type, command.gesture_id]),
  };
  vm.runInNewContext(fs.readFileSync(path.join(__dirname, 'button_scene_runtime.js'), 'utf8'), {
    window, document: { getElementById: id => (id === 'button-container' ? container : null),
      querySelector: () => null },
    performance: { now: () => 0 }, requestAnimationFrame() {},
  });
  const card = (id, x = 0) => ({ id, visible: true, rect: { x, y: 0, width: 100, height: 50 },
    control_rect: { x, y: 0, width: 108, height: 54 }, controls: { opacityPercent: 100 } });
  return { window, log, layouts, container, card, scene: window.__SGT_BUTTON_SCENE__,
    send: command => window.applyHostCommand(command), clear: () => { log.length = 0; } };
}

test('batched card creation populates the control scene in one layout', () => {
  const h = harness();
  h.send({ type: 'upsert_batch', cards: [h.card(1), h.card(2)] });
  assert.deepEqual(h.layouts.at(-1) && Object.keys(h.layouts.at(-1)), ['1', '2']);
  assert.deepEqual(h.log.filter(entry => entry[0] === 'result').map(entry => entry[1]), ['upsert_batch']);
});

test('drag hides controls and clears native regions after invalidating the visibility cache', () => {
  const h = harness();
  h.send({ type: 'upsert', card: h.card(1) });
  h.clear();
  h.scene.setDragActive(true, 7);
  assert.equal(h.container.style.visibility, 'hidden');
  assert.deepEqual(h.log.map(entry => entry[0]), ['invalidate', 'ipc']);
  assert.equal(h.log[1][1].action, 'update_clickable_regions');
  assert.deepEqual(h.log[1][1].regions, []);
  assert.equal(h.scene.isGeometryPreviewActive(), true);
  h.clear();
  h.send({ type: 'geometry', cards: [h.card(1, 40)] });
  assert.equal(h.log.some(entry => entry[0] === 'layout'), false, 'no layout while dragging');
});

test('release keeps controls hidden until the matching settlement lands committed geometry', () => {
  const h = harness();
  h.send({ type: 'upsert', card: h.card(1) });
  h.scene.setDragActive(true, 7);
  h.clear();
  h.scene.releaseDragPreview(320, 240, 7);
  assert.deepEqual(h.log, [['cursor', 320, 240]]);
  assert.equal(h.container.style.visibility, 'hidden');
  assert.equal(h.scene.isGeometryPreviewActive(), true);

  h.send({ type: 'drag_settled', gesture_id: 99, cards: [h.card(1, 900)] });
  assert.deepEqual(h.log, [['cursor', 320, 240]], 'a stale settlement reaches neither scene');

  h.send({ type: 'drag_settled', gesture_id: 7, cards: [h.card(1, 60)] });
  const order = h.log.map(entry => entry[0]);
  assert.ok(order.indexOf('unlock') < order.indexOf('result'), 'preview lock releases before results apply');
  assert.ok(order.indexOf('result') < order.indexOf('layout'), 'results apply before controls return');
  assert.ok(order.indexOf('layout') < order.indexOf('restore'));
  assert.deepEqual(h.log.find(entry => entry[0] === 'restore'), ['restore', 7]);
  assert.equal(h.layouts.at(-1)['1'].rect.x, 60, 'controls lay out at the settled geometry');
  assert.equal(h.container.style.visibility, '');
  assert.equal(h.scene.isGeometryPreviewActive(), false);
});

test('an unmatched release cannot end a drag', () => {
  const h = harness();
  h.scene.setDragActive(true, 3);
  h.scene.releaseDragPreview(1, 1, 4);
  h.scene.setDragActive(false, 4);
  assert.equal(h.container.style.visibility, 'hidden');
  assert.equal(h.scene.isGeometryPreviewActive(), true);
  h.scene.setDragActive(false, 3);
  assert.equal(h.container.style.visibility, '');
});
