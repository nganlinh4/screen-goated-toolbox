const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const test = require('node:test');
const assert = require('node:assert/strict');

function harness() {
  let session = '1';
  const handlers = {}, messages = [], timers = new Map();
  let nextTimer = 0;
  const parent = {realtimeSession: () => session, realtimePostMessage: (role, body) => messages.push([role, body])};
  const window = {parent, addEventListener() {}};
  const document = {addEventListener: (name, handler) => { (handlers[name] ||= []).push(handler); }};
  vm.runInNewContext(fs.readFileSync(path.join(__dirname, 'card_session.js'), 'utf8')
    .replace('__SGT_REALTIME_ROLE__', '"transcription"'), {window, document,
      setTimeout: callback => { const id = ++nextTimer; timers.set(id, callback); return id; },
      clearTimeout: id => timers.delete(id)});
  function fire(name, details = {}) {
    const event = {isTrusted: true, detail: 1, pointerId: 1, code: 'Enter', prevented: false, stopped: false,
      preventDefault() { this.prevented = true; }, stopImmediatePropagation() { this.stopped = true; }, ...details};
    for (const handler of handlers[name] || []) handler(event);
    return event;
  }
  return {window, messages, fire, flush: () => { for (const callback of timers.values()) callback(); timers.clear(); }, setSession: value => { session = value; window.__SGT_REALTIME_SESSION_CHANGED__(); }};
}

test('a pointer press cannot activate a control after its session is replaced', () => {
  const h = harness();
  h.fire('pointerdown');
  h.setSession('2');
  h.fire('keydown', {code: 'KeyA'});
  assert.equal(h.fire('input').stopped, false);
  h.fire('pointerup');
  h.flush();
  const oldClick = h.fire('click');
  assert.equal(oldClick.prevented && oldClick.stopped, true);
  h.fire('pointerdown'); h.fire('pointerup');
  assert.equal(h.fire('click').stopped, false);
  assert.equal(h.fire('change').stopped, false);
  h.window.realtimePostMessage('close');
  assert.deepEqual(h.messages, [['transcription', 'close']]);
});

test('keyboard activation and untrusted programmatic events keep their own boundaries', () => {
  const h = harness();
  h.fire('keydown');
  h.setSession('2');
  assert.equal(h.fire('keydown', {repeat: true}).stopped, true);
  assert.equal(h.fire('click', {detail: 0}).stopped, true);
  assert.equal(h.fire('click', {detail: 0, isTrusted: false}).stopped, false);
  h.flush();
  assert.equal(h.fire('click', {detail: 0}).stopped, false);
  assert.equal(h.fire('input').stopped, false);
  h.fire('keydown');
  assert.equal(h.fire('click', {detail: 0}).stopped, false);
  h.fire('compositionstart');
  assert.equal(h.fire('input').stopped, false);
});

test('IME composition started in an old session cannot edit its replacement', () => {
  const h = harness();
  h.fire('compositionstart'); h.flush();
  h.setSession('2');
  assert.equal(h.fire('beforeinput', {isComposing: true}).stopped, true);
  assert.equal(h.fire('input', {isComposing: true}).stopped, true);
  h.fire('compositionend');
  assert.equal(h.fire('input').stopped, true);
  h.flush();
  assert.equal(h.fire('input', {inputType: 'insertFromComposition'}).stopped, true);
  h.fire('compositionstart');
  assert.equal(h.fire('beforeinput').stopped, false);
});

test('text edits rebuild the shown text from the snapshot and only the change', () => {
  const h = harness();
  const shown = [];
  h.window.updateText = (committed, draft) => shown.push([committed, draft]);
  h.window.setRealtimeText('Xin chào 😀', ' draft');
  h.window.editRealtimeText(0, ' one', ' next');
  h.window.editRealtimeText(6, 'x', '');
  assert.deepEqual(shown.at(-1), ['Xin chào x', '']);
  h.window.editRealtimeText(0, ' 한국어', '');
  assert.deepEqual(shown.at(-1), ['Xin chào x 한국어', '']);
  assert.throws(() => h.window.editRealtimeText(100, '', ''));
  assert.deepEqual(shown.at(-1), ['Xin chào x 한국어', ''], 'a mismatched edit changes nothing');
});
