const { test } = require('node:test');
const assert = require('node:assert/strict');
const { readFileSync } = require('node:fs');
const { join } = require('node:path');
const vm = require('node:vm');

function harness(reduced = false) {
  const frames = [];
  const scope = vm.createContext({
    Element: { prototype: { animate() {} } },
    window: { matchMedia: () => ({ matches: reduced }) },
    requestAnimationFrame: callback => frames.push(callback),
  });
  vm.runInContext(readFileSync(join(__dirname, 'settled_reveal_runtime.js'), 'utf8')
    + '\nglobalThis.reveal = sourceReplacementReveal;', scope);
  function element() {
    return {
      isConnected: true, style: { willChange: '' }, animations: [],
      animate(keys, options) {
        const listeners = {};
        const animation = { keys, options, cancelled: false,
          addEventListener: (event, callback) => { listeners[event] = callback; },
          finish: () => listeners.finish?.(),
          cancel() { this.cancelled = true; listeners.cancel?.(); },
        };
        this.animations.push(animation);
        return animation;
      },
    };
  }
  return {
    reveal: scope.reveal,
    entry: ready => ({ visualSurface: element(), directHost: element(), card: element(),
      contentRevision: 1, directState: { sourceLayoutReady: ready } }),
    async tick() {
      for (const callback of frames.splice(0)) callback();
      for (let i = 0; i < 8; i++) await Promise.resolve();
    },
  };
}

test('a ready card reveals independently; only its text blurs and moves', async () => {
  const h = harness();
  const slow = h.entry(new Promise(() => {}));
  const ready = h.entry(Promise.resolve());
  let painted = 0;
  h.reveal.enqueue(slow, () => assert.fail('unready card painted'));
  h.reveal.enqueue(ready, () => painted++);
  await h.tick(); await h.tick();
  assert.equal(slow.visualSurface.animations.length, 0);
  assert.equal(ready.visualSurface.animations.length, 1);
  assert.equal(ready.visualSurface.animations[0].keys[0].filter, undefined);
  const text = ready.directHost.animations[0];
  assert.equal(text.keys[0].filter, 'blur(8px)');
  assert.equal(text.options.duration, 350);
  text.finish();
  assert.equal(painted, 1);
  assert.equal(ready.visualSurface.style.opacity, '1');
  assert.equal(ready.directHost.style.willChange, '');
  assert.equal(text.cancelled, true);
});

test('reduced motion reveals without animation', async () => {
  const h = harness(true), entry = h.entry(Promise.resolve());
  let painted = 0;
  h.reveal.enqueue(entry, () => painted++);
  await h.tick(); await h.tick();
  assert.equal(painted, 1);
  assert.equal(entry.directHost.animations.length, 0);
  assert.equal(entry.visualSurface.style.opacity, '1');
});

test('cancel and stale revisions cannot reveal or report old content', async () => {
  const h = harness(), entry = h.entry(Promise.resolve());
  h.reveal.enqueue(entry, () => assert.fail('stale paint'));
  entry.contentRevision++;
  await h.tick(); await h.tick();
  assert.equal(entry.directHost.animations.length, 0);
  h.reveal.enqueue(entry, () => assert.fail('cancelled paint'));
  await h.tick(); await h.tick();
  h.reveal.cancel(entry);
  assert.equal(entry.directHost.animations[0].cancelled, true);
  assert.equal(entry.visualSurface.animations[0].cancelled, true);
  assert.equal(entry.sourceReplacementReveal, null);
});
