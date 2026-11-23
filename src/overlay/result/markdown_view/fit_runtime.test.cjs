// Runs the shipped result fitter (fit_font_script_part1/2.js, composed the
// same way as markdown_view::fit::runtime_fit_script) against a small text
// layout model, so the tests observe chosen sizes instead of source text.
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const test = require('node:test');
const assert = require('node:assert/strict');

const FIT_SOURCE = ['fit_font_script_part1.js', 'fit_font_script_part2.js']
  .map(name => fs.readFileSync(path.join(__dirname, 'streaming', 'fit_impl', name), 'utf8'))
  .join('')
  .replaceAll('__FIT_PHASE__', 'fit_font_to_window_runtime')
  .replaceAll('__STREAMING_MODE__', 'Boolean(streaming)');

// Monospace-like model: each character is half an em wide at 100% stretch,
// lines wrap at the viewport width and stack at font size times line height.
function layout(body, viewport) {
  const size = parseFloat(body.style.fontSize) || 16;
  const stretch = (parseFloat(body.style.fontStretch) || 100) / 100;
  const lineHeight = parseFloat(body.style.lineHeight) || 1.15;
  const spacing = parseFloat(body.style.letterSpacing) || 0;
  const charWidth = size * 0.5 * stretch + spacing;
  const perLine = Math.max(1, Math.floor(viewport.clientWidth / charWidth));
  const lines = Math.max(1, Math.ceil(body.visibleText.length / perLine));
  const top = parseFloat(body.style.paddingTop) || 0;
  const textHeight = lines * size * lineHeight;
  const longestWord = Math.max(...body.visibleText.split(/\s+/).map(word => word.length));
  return {
    textTop: top,
    textBottom: top + textHeight,
    height: top + textHeight + (parseFloat(body.style.paddingBottom) || 0),
    width: Math.max(viewport.clientWidth, Math.ceil(longestWord * charWidth)),
  };
}

function fitsAt(visibleText, viewport, fontSize, fontStretch = '90%') {
  const body = { visibleText, style: { fontSize: fontSize + 'px', fontStretch, lineHeight: '1.15' } };
  const box = layout(body, viewport);
  return box.height <= viewport.clientHeight && box.width <= viewport.clientWidth + 1;
}

function makeBody(visibleText, hiddenScript = '') {
  const style = {
    setProperty(name, value, priority) { this[name] = value; this[name + ':priority'] = priority; },
    removeProperty(name) { delete this[name]; },
  };
  const textNodes = [{ nodeValue: visibleText, parentElement: { tagName: 'P' } }];
  if (hiddenScript) textNodes.push({ nodeValue: hiddenScript, parentElement: { tagName: 'SCRIPT' } });
  return {
    style, visibleText, textNodes, children: [],
    textContent: visibleText + hiddenScript,
    innerText: visibleText,
    innerHTML: visibleText + hiddenScript,
    get offsetHeight() { return 0; },
    querySelector: () => null,
    querySelectorAll: () => [],
    appendChild(child) { this.children.push(child); },
    getBoundingClientRect: () => ({ left: 0, top: 0, right: 0, bottom: 0 }),
  };
}

function run({ text, script = '', width = 400, height = 200, streaming = false,
  settleBeforeReveal = true, fontReady = true, streamingSession = false, body, state }) {
  body = body || makeBody(text, script);
  state = state || {};
  const viewport = { clientWidth: width, clientHeight: height };
  const frames = [];
  const completions = [];
  const diagnostics = [];
  let now = 0;
  Object.defineProperty(body, 'scrollHeight', { configurable: true, get: () => layout(body, viewport).height });
  Object.defineProperty(body, 'scrollWidth', { configurable: true, get: () => layout(body, viewport).width });
  const context = vm.createContext({
    streaming,
    performance: { now: () => now },
    NodeFilter: { SHOW_TEXT: 4 },
    requestAnimationFrame: () => { throw new Error('fit context frames must be used'); },
    cancelAnimationFrame: () => {},
    document: {
      fonts: { check: () => false, status: 'loading' },
      createTreeWalker(root) {
        let index = 0;
        return { nextNode: () => root.textNodes[index++] || null };
      },
      createRange: () => ({
        selectNodeContents() {},
        getBoundingClientRect() {
          const box = layout(body, viewport);
          return { left: 0, top: box.textTop, right: Math.min(box.width, viewport.clientWidth), bottom: box.textBottom };
        },
      }),
    },
    window: {
      __SGT_FIT_CONTEXT__: {
        state, body, viewport, settleBeforeReveal, fontReady, streamingSession,
        inAnimationFrame: false,
        scheduleFrame: callback => { frames.push(callback); return frames.length; },
        cancelFrame: () => {},
        isCurrent: () => true,
        complete: refine => completions.push(refine),
        reportDiagnostic: payload => diagnostics.push(payload),
      },
    },
  });
  vm.runInContext(FIT_SOURCE, context);
  for (let guard = 0; frames.length && guard < 10_000; guard++) {
    now += 16;
    frames.shift()(now);
  }
  return { body, state, completions, diagnostics, fontSize: parseFloat(body.style.fontSize), viewport };
}

const SENTENCE = 'The quick brown fox jumps over the lazy dog. ';

test('a final fit picks the largest size whose text stays inside the viewport', () => {
  const text = SENTENCE.repeat(4).trim();
  const result = run({ text });
  assert.deepEqual(result.completions, [false]);
  assert.ok(fitsAt(text, result.viewport, result.fontSize), `${result.fontSize}px overflows`);
  assert.ok(!fitsAt(text, result.viewport, result.fontSize * 1.05), `${result.fontSize}px leaves room`);
  assert.equal(result.body.style.overflowY, 'hidden');
});

test('longer results settle smaller and still fit', () => {
  const short = run({ text: SENTENCE.repeat(2).trim() });
  const long = run({ text: SENTENCE.repeat(8).trim() });
  assert.ok(long.fontSize < short.fontSize, `${long.fontSize} >= ${short.fontSize}`);
  assert.ok(fitsAt(SENTENCE.repeat(8).trim(), long.viewport, long.fontSize));
});

test('embedded script text never shrinks the measured result', () => {
  const text = SENTENCE.repeat(3).trim();
  const plain = run({ text, width: 200, height: 60 });
  const scripted = run({ text, script: 'x'.repeat(20_000), width: 200, height: 60 });
  assert.equal(scripted.fontSize, plain.fontSize);
  assert.ok(plain.fontSize < 14, 'fixture must sit below the long-text readability floor');
  assert.equal(scripted.body.style.overflowY, 'hidden');
});

test('a hidden final render is revealed at its target without interpolation', () => {
  const text = SENTENCE.repeat(4).trim();
  const body = makeBody(text);
  body.style.fontSize = '40px';
  const result = run({ text, body, settleBeforeReveal: true });
  assert.equal(result.body.style.opacity, '1');
  assert.equal(result.body.style['opacity:priority'], 'important');
  const target = result.diagnostics.find(payload => payload.action === 'fit_target');
  assert.equal(target.settleBeforeReveal, true);
  assert.equal(target.fromFontSize, 40);
  assert.equal(result.fontSize, target.fontSize);
});

test('streaming fits cap early growth even in a tall viewport', () => {
  const result = run({ text: 'Hi there', width: 900, height: 700, streaming: true, settleBeforeReveal: false });
  assert.ok(result.fontSize <= 48, `${result.fontSize}px`);
  assert.ok(result.fontSize >= 40, `${result.fontSize}px`);
});

test('a missing required font completes without touching the layout', () => {
  const result = run({ text: SENTENCE, fontReady: false });
  assert.deepEqual(result.completions, [undefined]);
  assert.equal(result.body.style.fontSize, undefined);
  assert.ok(result.diagnostics.some(payload => payload.reason === 'required_font_unavailable'));
});

test('an unchanged final fit is not measured again', () => {
  const text = SENTENCE.repeat(4).trim();
  const first = run({ text });
  const probes = first.diagnostics.find(payload => payload.action === 'fit_target').layoutProbes;
  assert.ok(probes > 0);
  const again = run({ text, body: first.body, state: first.state });
  assert.equal(again.fontSize, first.fontSize);
  assert.equal(again.diagnostics.some(payload => payload.action === 'fit_target'), false);
});
