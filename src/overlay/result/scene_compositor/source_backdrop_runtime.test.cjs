const { test } = require('node:test');
const assert = require('node:assert/strict');
const { readFileSync } = require('node:fs');
const { join } = require('node:path');
const vm = require('node:vm');

function harness() {
  function element(connected = false) {
    return {
      style: { setProperty() {} }, dataset: {}, children: [], hidden: false,
      get isConnected() { return connected || !!this.parent?.isConnected; },
      remove() {
        if (this.parent) this.parent.children.splice(this.parent.children.indexOf(this), 1);
        this.parent = null;
      },
      appendChild(child) { child.remove(); child.parent = this; this.children.push(child); },
      prepend(child) { child.remove(); child.parent = this; this.children.unshift(child); },
      insertBefore(child, before) {
        child.remove(); child.parent = this;
        this.children.splice(this.children.indexOf(before), 0, child);
      },
    };
  }
  const scene = element(true), cards = new Map();
  const scope = vm.createContext({
    window: {}, scene, cards, document: { createElement: () => element() },
    clearTimeout() {}, cancelActiveFit() {},
    sourceReplacementReveal: { cancel() {} }, __SGT_BOX_RADIUS_PX__: 12,
  });
  for (const file of ['surface_runtime.js', 'scene_command_helpers.js', 'host_command_runtime.js']) {
    vm.runInContext(readFileSync(join(__dirname, file), 'utf8'), scope);
  }
  function entry(id, color) {
    const card = element(), visualSurface = element(), backdrop = element();
    card.dataset.id = id; backdrop.dataset.url = color;
    const value = { card, visualSurface, backdrop, visible: true,
      directHost: element(), frame: element(), processing: { element: element(), resize() {}, destroy() {} },
      directRuntime: { destroy() {} }, resizeRuntime: { destroy() {} } };
    for (const child of [backdrop, value.directHost, value.frame, value.processing.element]) card.appendChild(child);
    scene.appendChild(card); cards.set(id, value);
    scope.setSourceReplacementSurface(value, true, color);
    return value;
  }
  return { scope, scene, cards, entry };
}

test('source patches share a lower plane without flattening colors or exposing prewarm', () => {
  const h = harness(), a = h.entry('1', 'warm-pixels'), b = h.entry('2', 'cool-pixels');
  const plane = h.scene.children[0];
  assert.equal(plane.className, 'source-backdrops');
  assert.equal(a.backdrop.parent.parent, plane);
  assert.equal(b.backdrop.parent.parent, plane);
  assert.equal(a.directHost.parent, a.visualSurface);
  assert.equal(a.visualSurface.parent, a.card);
  assert.equal(a.backdrop.dataset.url, 'warm-pixels');
  assert.equal(b.backdrop.dataset.url, 'cool-pixels');
  assert.equal(a.backdrop.style.opacity, '0');
  assert.equal(a.visualSurface.style.maskImage, 'url("warm-pixels")');
});

test('geometry, drag preview, opacity and hidden state keep both planes registered', () => {
  const h = harness(), a = h.entry('1', 'pixels');
  h.scope.applyGeometry(a, { rect: { x: -200, y: 30, width: 400, height: 80 } });
  assert.equal(a.sourceBackdropSurface.style.transform, 'translate3d(-200px,30px,0)');
  assert.equal(a.sourceBackdropSurface.style.width, '400px');
  a.card.style.translate = '14px -8px'; a.card.hidden = true;
  h.scope.window.__SGT_SYNC_SOURCE_BACKDROP__('1');
  assert.equal(a.sourceBackdropSurface.style.translate, '14px -8px');
  assert.equal(a.sourceBackdropSurface.hidden, true);
  for (const opacity of [0, 55, 100, 0, 100]) {
    h.scope.window.applyHostCommand({ type: 'opacity', id: '1', opacity });
    assert.equal(a.sourceBackdropSurface.style.opacity, String(opacity / 100));
    assert.equal(a.card.style.opacity, String(opacity / 100));
  }
  a.visible = false;
  h.scope.window.applyHostCommand({ type: 'opacity', id: '1', opacity: 100 });
  assert.equal(a.sourceBackdropSurface.style.opacity, '0');
});

test('conversion and removal leave no orphan background patches', () => {
  const h = harness(), a = h.entry('1', 'first'), b = h.entry('2', 'second');
  const plane = h.scene.children[0];
  h.scope.setSourceReplacementSurface(a, false, '');
  assert.equal(a.backdrop.parent, a.card);
  assert.equal(a.backdrop.style.opacity, '');
  assert.equal(a.sourceBackdropSurface, null);
  assert.equal(a.visualSurface.isConnected, false);
  assert.equal(plane.children.length, 1);
  h.scope.removeCard('2');
  assert.equal(plane.children.length, 0);
  assert.equal(b.card.isConnected, false);
  assert.equal(h.cards.has('2'), false);
});
