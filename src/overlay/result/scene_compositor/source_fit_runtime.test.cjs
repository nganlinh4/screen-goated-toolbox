const { test } = require('node:test');
const assert = require('node:assert/strict');
const { readFileSync } = require('node:fs');
const { join } = require('node:path');
const vm = require('node:vm');

function harness() {
  const frames = [];
  let elapsed = 0;
  const rect = { left: 0, top: 0, width: 40, height: 16 };
  const scope = vm.createContext({
    window: {}, performance: { now: () => elapsed },
    requestAnimationFrame: callback => frames.push(callback),
    document: { createRange: () => ({
      selectNodeContents() {},
      getBoundingClientRect() { elapsed += 1; return rect; },
    }) },
  });
  vm.runInContext(readFileSync(join(__dirname, 'surface_runtime.js'), 'utf8'), scope);
  function item(connected = true) {
    return {
      wrap: false, vertical: false,
      box: { isConnected: connected, clientWidth: 100, clientHeight: 30,
        style: {}, getBoundingClientRect: () => ({ ...rect, width: 100, height: 30 }) },
      text: { isConnected: connected, textContent: 'Complete text 123',
        style: {}, setAttribute() {}, getBoundingClientRect: () => rect },
    };
  }
  return {
    queue: scope.window.__SGT_QUEUE_SOURCE_FIT__, item,
    get pendingFrames() { return frames.length; },
    async tick() {
      const callback = frames.shift();
      assert.ok(callback, 'a continuation frame is scheduled');
      callback();
      await Promise.resolve();
    },
  };
}

test('dense fitting yields while completed regions become ready independently', async () => {
  const h = harness();
  let completed = 0;
  const items = Array.from({ length: 80 }, () => h.item());
  const promises = items.map(item => h.queue([item]).then(() => completed++));
  await h.tick();
  assert.ok(completed > 0 && completed < items.length);
  while (h.pendingFrames) await h.tick();
  await Promise.all(promises);
  assert.equal(completed, items.length);
  for (const item of items) {
    assert.equal(item.text.textContent, 'Complete text 123');
    assert.equal(item.box.clientWidth, 100);
    assert.equal(item.box.clientHeight, 30);
    assert.equal(item.box.style.overflow, 'hidden');
  }
});

test('a multi-region task resolves only after all of its regions are fitted', async () => {
  const h = harness();
  let done = false;
  const pending = h.queue(Array.from({ length: 80 }, () => h.item())).then(() => { done = true; });
  await h.tick();
  assert.equal(done, false);
  while (h.pendingFrames) await h.tick();
  await pending;
  assert.equal(done, true);
});

test('empty and detached work completes without mutating detached content', async () => {
  const h = harness();
  const detached = h.item(false);
  const pending = Promise.all([h.queue([]), h.queue([detached])]);
  while (h.pendingFrames) await h.tick();
  await pending;
  assert.deepEqual(detached.text.style, {});
});
