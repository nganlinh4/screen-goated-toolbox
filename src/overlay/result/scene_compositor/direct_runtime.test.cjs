// The shared direct card runtime: content application, inline sizing and
// teardown, run against a DOM whose text metrics follow a simple glyph model.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { createRequire } = require('node:module');
const frontendRequire = createRequire(require.resolve('../../../../screen-record/package.json'));
const { JSDOM } = frontendRequire('jsdom');

const read = name => fs.readFileSync(path.join(__dirname, name), 'utf8');

// Glyphs are half an em wide at 100% stretch; wrapping is per word, and a
// word wider than the line breaks between its glyphs, as the card CSS allows.
function measure(body, width) {
  const fontSize = parseFloat(body.style.fontSize) || 14;
  const stretch = (parseFloat(body.style.fontStretch) || 100) / 100;
  const lineHeight = body.style.lineHeight.endsWith('px') ? parseFloat(body.style.lineHeight)
    : fontSize * (parseFloat(body.style.lineHeight) || 1.5);
  const glyph = fontSize * 0.5 * stretch;
  const words = (body.textContent || '').trim().split(/\s+/).filter(Boolean);
  let lines = words.length ? 1 : 0;
  let used = 0;
  let widest = 0;
  for (const word of words) {
    const size = word.length * glyph;
    const next = used ? used + glyph + size : size;
    if (next <= width) { used = next; continue; }
    if (used) lines++;
    if (size <= width) { used = size; continue; }
    const perLine = Math.floor(width / glyph);
    if (perLine < 1) { widest = Math.max(widest, glyph); lines += word.length - 1; used = glyph; continue; }
    lines += Math.ceil(word.length / perLine) - 1;
    used = (word.length % perLine || perLine) * glyph;
  }
  return { height: lines * lineHeight, width: Math.max(width, widest) };
}

function harness({ width = 300, height = 120 } = {}) {
  const dom = new JSDOM('<div id="card"></div>', { runScripts: 'dangerously', pretendToBeVisual: true });
  const { window } = dom;
  const document = window.document;
  const frames = [];
  const cancelled = [];
  window.requestAnimationFrame = callback => frames.push(callback);
  window.cancelAnimationFrame = handle => cancelled.push(handle);
  // Like a browser, computed line height is resolved to pixels.
  const computed = window.getComputedStyle.bind(window);
  window.getComputedStyle = element => {
    if (element !== body) return computed(element);
    const fontSize = parseFloat(body.style.fontSize) || 14;
    const lineHeight = body.style.lineHeight.endsWith('px') ? parseFloat(body.style.lineHeight)
      : fontSize * (parseFloat(body.style.lineHeight) || 1.5);
    return { fontSize: fontSize + 'px', lineHeight: lineHeight + 'px' };
  };
  for (const script of ['shape_runtime.js', 'dom_patch_runtime.js', 'reveal_runtime.js', 'direct_runtime.js']) {
    window.eval(read(script));
  }
  const card = document.getElementById('card');
  card.style.width = width + 'px';
  card.style.height = height + 'px';
  const body = document.createElement('div');
  card.appendChild(body);
  Object.defineProperty(body, 'scrollHeight', { get: () => measure(body, width).height });
  Object.defineProperty(body, 'scrollWidth', { get: () => measure(body, width).width });
  const backdrop = document.createElement('img');
  const entry = {
    card, bodyElement: body, backdrop,
    directState: { wordCount: 0, renderCount: 0, overflowObserver: null, fit: {},
      reveal: { queue: [], active: false, lastRevealedIndex: -1, lastTick: 0, credits: 0, generation: 0 } },
  };
  const runtime = window.__SGT_CREATE_DIRECT_RUNTIME__(entry, { requestFit() {}, diagnostic() {} });
  const fits = () => measure(body, width).height <= height + 2 && measure(body, width).width <= width + 2;
  return { window, document, body, entry, runtime, frames, cancelled, fits, width, height };
}

const ordinary = (html, extra = {}) => ({ html, runInlineSizing: true, animateNewWords: false, ...extra });

test('ordinary content is patched in place and keeps a scroll recovery path', () => {
  const h = harness();
  h.runtime.apply(ordinary('<p>first line</p>'));
  const paragraph = h.body.querySelector('p');
  h.runtime.apply(ordinary('<p>first line</p><p>more streamed text</p>'));
  assert.equal(h.body.querySelector('p'), paragraph, 'unchanged nodes survive a streamed update');
  assert.equal(h.body.style.overflowY, 'auto');
  assert.equal(h.body.style.overflowX, 'hidden');
  assert.equal(h.body.style.opacity, '1');
});

test('a new ordinary session picks the largest readable size that fits', () => {
  const h = harness();
  h.runtime.apply(ordinary('<p>Short translated sentence for the card</p>', { finalizing: true }));
  const size = parseFloat(h.body.style.fontSize);
  assert.equal(h.body.style.fontStretch, '90%');
  assert.equal(h.body.style.lineHeight, '1.5');
  assert.ok(h.fits(), `size ${size} overflows`);
  h.body.style.fontSize = (size + 1) + 'px';
  assert.ok(!h.fits(), `size ${size} is not the largest fitting size`);
});

test('source replacements are sanitized, tightly set and prefer a wide axis over condensing', () => {
  const h = harness({ width: 400, height: 40 });
  h.runtime.apply(ordinary('<p onclick="x()">Replacement text</p>', {
    sourceReplacement: true, preferredFontSize: 18, finalizing: true,
  }));
  assert.equal(h.body.querySelector('[onclick]'), null);
  assert.equal(h.body.style.lineHeight, '1.08');
  assert.equal(h.body.style.overflowY, '', 'replacements never scroll');
  assert.equal(h.body.style.fontSize, '18px', 'a final replacement keeps, never exceeds, its source size');
  assert.equal(h.body.style.fontStretch, '100%', 'no condensing when the full width fits');
  assert.equal(h.body.style.justifyContent, 'center');
  assert.equal(h.body.style.getPropertyValue('animation'), 'none');
  assert.equal(h.body.style.getPropertyPriority('opacity'), 'important');
});

test('a source replacement condenses before it shrinks when the text is too wide', () => {
  const h = harness({ width: 150, height: 20 });
  h.runtime.apply(ordinary('<p>Overlong replacement</p>', { sourceReplacement: true, preferredFontSize: 16 }));
  const stretch = parseFloat(h.body.style.fontStretch);
  assert.ok(stretch < 100 && stretch >= 25, `stretch ${stretch}`);
  assert.ok(h.fits());
});

test('pathological one-letter-per-line wraps are rejected in favour of a smaller size', () => {
  const h = harness({ width: 60, height: 400 });
  const text = 'abcdefgh ijklmnop';
  h.runtime.apply(ordinary(`<p>${text}</p>`, { sourceReplacement: true, preferredFontSize: 40, finalizing: true }));
  const lines = Math.round(h.body.scrollHeight / (parseFloat(h.body.style.fontSize) * 1.08));
  // A few glyphs per line across more lines than words is a broken word stack.
  assert.ok(text.length / lines >= 3.5 || lines <= 3, `${lines} lines for ${text.length} glyphs`);
  assert.ok(h.fits());
});

test('source regions lay out scaled slots and queue one source fit with their footprints', async () => {
  const h = harness({ width: 200, height: 100 });
  let queued;
  h.window.__SGT_QUEUE_SOURCE_FIT__ = items => { queued = items; return Promise.resolve('ready'); };
  Object.defineProperty(h.entry.backdrop, 'complete', { get: () => true });
  Object.defineProperty(h.entry.backdrop, 'naturalWidth', { get: () => 400 });
  Object.defineProperty(h.entry.backdrop, 'naturalHeight', { get: () => 200 });
  h.entry.backdrop.dataset.url = 'data:image/png;base64,AA==';
  h.runtime.apply(ordinary('<p>one two</p>', {
    sourceReplacement: true, preferredFontSize: 12,
    sourceRegions: [{ x: 20, y: 40, width: 100, height: 20, footprint: [[0, 0, 10, 10]] },
      { x: 200, y: 0, width: 20, height: 100, vertical: true }],
    sourceSegments: ['one', 'two'],
  }));
  const slots = h.body.querySelectorAll('.source-line');
  assert.equal(slots.length, 2);
  assert.deepEqual([slots[0].style.left, slots[0].style.top, slots[0].style.width], ['10px', '20px', '50px']);
  assert.equal(slots[1].firstChild.style.writingMode, 'vertical-rl');
  assert.deepEqual([...queued.map(item => item.text.textContent)], ['one', 'two']);
  assert.deepEqual(JSON.parse(JSON.stringify(queued[0].footprint)), [[0, 0, 5, 5]]);
  assert.equal(queued[1].vertical, true);
  assert.equal(queued[0].preferredFontSize, 12);
  assert.equal(h.body.dataset.shapeLayout, 'true');
  assert.equal(await h.entry.directState.sourceLayoutReady, 'ready');
});

test('a replacement waits for its backdrop to decode before it lays out', async () => {
  const h = harness({ width: 200, height: 100 });
  let decoded;
  let queued = 0;
  h.window.__SGT_QUEUE_SOURCE_FIT__ = () => { queued++; return Promise.resolve(); };
  let complete = false;
  Object.defineProperty(h.entry.backdrop, 'complete', { get: () => complete });
  Object.defineProperty(h.entry.backdrop, 'naturalWidth', { get: () => (complete ? 200 : 0) });
  h.entry.backdrop.decode = () => new Promise(resolve => { decoded = resolve; });
  h.entry.backdrop.dataset.url = 'data:image/png;base64,AA==';
  h.runtime.apply(ordinary('<p>text</p>', { sourceReplacement: true, preferredFontSize: 12,
    sourceRegions: [{ x: 0, y: 0, width: 50, height: 20 }], sourceSegments: ['text'] }));
  assert.equal(queued, 0, 'no layout against an undecoded backdrop');
  complete = true;
  decoded();
  await h.entry.directState.sourceLayoutReady;
  assert.equal(queued, 1);
  assert.equal(h.body.style.opacity, '1');
});

test('destroy stops persistent scale motion and the overflow guard', () => {
  const h = harness();
  const disconnected = [];
  h.entry.directState.fit._sgtMotionController = { frame: 17 };
  h.entry.directState.overflowObserver = { disconnect: () => disconnected.push(true) };
  h.runtime.destroy();
  assert.deepEqual(h.cancelled, [17]);
  assert.equal(h.entry.directState.fit._sgtMotionController, null);
  assert.deepEqual(disconnected, [true]);
  assert.equal(h.entry.directState.overflowObserver, null);
});

test('vertical writing is preferred only for mostly Han and kana text', () => {
  const h = harness();
  const shape = h.window.__SGT_SHAPE_LAYOUT__;
  assert.equal(shape.prefersVerticalWriting('縦書きのテキスト'), true);
  assert.equal(shape.prefersVerticalWriting('漢字 and some English words'), false);
  assert.equal(shape.prefersVerticalWriting('...'), false);
});
