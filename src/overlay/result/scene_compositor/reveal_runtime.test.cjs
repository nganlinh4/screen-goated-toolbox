// Streamed-word reveal: a bounded, adaptive queue driven by animation frames.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

function harness(wordCount) {
  const frames = [];
  let now = 0;
  const body = { words: [], querySelectorAll: () => body.words };
  const addWords = count => {
    for (let index = 0; index < count; index++) {
      body.words.push({ isConnected: true, style: { visibility: 'visible', opacity: '1' } });
    }
  };
  addWords(wordCount);
  const window = {};
  vm.runInNewContext(fs.readFileSync(path.join(__dirname, 'reveal_runtime.js'), 'utf8'), {
    window, performance: { now: () => now },
    requestAnimationFrame: callback => frames.push(callback),
  });
  const reveal = { generation: 0, queue: [], active: false, lastRevealedIndex: -1 };
  const runtime = window.__SGT_CREATE_REVEAL_RUNTIME__(body, reveal);
  return {
    body, reveal, runtime, addWords, frames,
    visible: () => body.words.filter(word => word.style.visibility === 'visible').length,
    frame(milliseconds) {
      now += milliseconds;
      const pending = frames.splice(0);
      for (const callback of pending) callback(now);
    },
  };
}

test('a new session or a non-animated update shows every word at once', () => {
  const h = harness(5);
  h.runtime.update(true, true);
  assert.equal(h.visible(), 5);
  assert.equal(h.frames.length, 0);
  h.addWords(3);
  h.runtime.update(false, false);
  assert.equal(h.visible(), 8);
  assert.equal(h.reveal.lastRevealedIndex, 7);
});

test('a burst never lags more than 80 words behind the stream', () => {
  const h = harness(0);
  h.runtime.update(true, true);
  h.addWords(300);
  h.runtime.update(true, false);
  assert.equal(h.reveal.queue.length, 80);
  assert.equal(h.visible(), 220);
  assert.equal(h.body.words[219].style.opacity, '1');
  assert.equal(h.body.words[220].style.visibility, 'hidden');
  assert.equal(h.body.words[220].style.transition, 'opacity 0.09s ease-out');
  assert.equal(h.body.words[220].style.filter, '', 'words fade without a blur');
});

test('reveal speed grows with the backlog and drains in order', () => {
  const small = harness(0);
  small.runtime.update(true, true);
  small.addWords(5);
  small.runtime.update(true, false);
  small.frame(100);
  const large = harness(0);
  large.runtime.update(true, true);
  large.addWords(60);
  large.runtime.update(true, false);
  large.frame(100);
  // 40 words/s at an empty queue; a deeper queue earns proportionally more.
  assert.ok(large.visible() > small.visible() * 2, `${large.visible()} vs ${small.visible()}`);
  for (let guard = 0; large.frames.length && guard < 100; guard++) large.frame(16);
  assert.equal(large.visible(), 60);
  assert.equal(large.reveal.lastRevealedIndex, 59);
  assert.equal(large.reveal.active, false);
});

test('appending words retargets the running loop instead of starting another', () => {
  const h = harness(0);
  h.runtime.update(true, true);
  h.addWords(10);
  h.runtime.update(true, false);
  assert.equal(h.frames.length, 1);
  h.addWords(10);
  h.runtime.update(true, false);
  assert.equal(h.frames.length, 1, 'one frame loop per card');
  assert.equal(h.reveal.queue.length, 20);
});

test('destroy stops an in-flight reveal from touching later words', () => {
  const h = harness(0);
  h.runtime.update(true, true);
  h.addWords(10);
  h.runtime.update(true, false);
  h.runtime.destroy();
  h.frame(1000);
  assert.equal(h.visible(), 0);
  assert.equal(h.frames.length, 0);
});
