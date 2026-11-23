import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { runInNewContext } from 'node:vm';

function wheelPage() {
  const source = readFileSync(new URL('../../src/overlay/preset_wheel/script.rs', import.meta.url), 'utf8');
  const start = source.indexOf('r#"') + 3;
  const script = source.slice(start, source.lastIndexOf('"#'));
  assert.ok(start >= 3 && script.length > 0);
  const sent = [], timers = new Map();
  let nextId = 0, now = 0;
  function schedule(fn, delay, repeat = false) {
    const id = ++nextId;
    timers.set(id, { fn, due: now + delay, delay, repeat });
    return id;
  }
  const element = () => ({ addEventListener() {}, innerHTML: '', innerText: '',
    classList: { add() {}, remove() {} }, getBoundingClientRect: () => ({ left: 0, right: 1, top: 0, bottom: 1 }) });
  const grid = element(), dismiss = element(), container = element();
  const window = { innerWidth: 1200, innerHeight: 700, ipc: { postMessage: (value) => sent.push(value) } };
  const context = { window, document: { getElementById: () => grid, querySelectorAll: () => [],
    querySelector: (selector) => selector === '.dismiss-btn' ? dismiss : container, addEventListener() {} },
    setTimeout: (fn, delay) => schedule(fn, delay), clearTimeout: (id) => timers.delete(id),
    setInterval: (fn, delay) => schedule(fn, delay, true), clearInterval: (id) => timers.delete(id),
    requestAnimationFrame: (fn) => schedule(fn, 0) };
  runInNewContext(script, context);
  function advance(ms) {
    const end = now + ms;
    while (true) {
      const next = [...timers].filter(([, timer]) => timer.due <= end).sort((a, b) => a[1].due - b[1].due)[0];
      if (!next) break;
      const [id, timer] = next;
      now = timer.due;
      if (timer.repeat) timer.due += timer.delay;
      else timers.delete(id);
      timer.fn();
    }
    now = end;
  }
  return { window, context, sent, timers, advance };
}

test('the actual wheel page stays live while the user chooses without input', () => {
  const page = wheelPage();
  page.window.updateContent('', 'Cancel', '1');
  page.advance(2100);
  assert.deepEqual(page.sent, ['ready_to_show:1', 'heartbeat:1', 'heartbeat:1']);
});

test('a replacement request fences old readiness and heartbeat callbacks', () => {
  const page = wheelPage();
  page.window.updateContent('', 'Cancel', '1');
  page.advance(0);
  const oldHeartbeat = [...page.timers.values()].find((timer) => timer.repeat)?.fn;
  assert.equal(typeof oldHeartbeat, 'function');
  page.sent.length = 0;
  page.window.updateContent('', 'Cancel', '2');
  oldHeartbeat();
  page.advance(1000);
  assert.deepEqual(page.sent, ['ready_to_show:2', 'heartbeat:2']);
  page.window.clearContent('1');
  page.advance(1000);
  assert.equal(page.sent.at(-1), 'heartbeat:2');
  page.window.clearContent('2');
  const before = page.sent.length;
  page.advance(10000);
  assert.equal(page.sent.length, before);
});

test('selection and dismissal stop the active heartbeat before native cleanup', () => {
  for (const event of ['select', 'dismiss']) {
    const page = wheelPage();
    page.window.updateContent('', 'Cancel', '1');
    page.advance(0);
    page.context[event](3);
    assert.equal(page.sent.at(-1), event === 'select' ? 'select:1:3' : 'dismiss:1');
    const before = page.sent.length;
    page.advance(10000);
    assert.equal(page.sent.length, before);
  }
});
