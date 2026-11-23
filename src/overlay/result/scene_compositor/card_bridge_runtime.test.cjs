// The bootstrap that card_bridge.rs injects into isolated result documents,
// run against a fake document: handshake, font gating, fit and frame relay,
// identity checks and diagnostics.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const bridge = fs.readFileSync(path.join(__dirname, 'card_bridge.rs'), 'utf8')
  .match(/const BRIDGE: &str = r#"<script>([\s\S]*?)<\/script>"#;/)[1];

function node(tag, extra = {}) {
  return {
    tagName: tag, dataset: {}, style: { setProperty(name, value) { this[name] = value; } }, children: [],
    classList: { names: new Set(), add(name) { this.names.add(name); } },
    appendChild(child) { this.children.push(child); return child; }, remove() { this.removed = true; },
    ...extra,
  };
}

function harness({ marker = 'sgt-card:42:3', bodyText = 'result', scriptText = '' } = {}) {
  const sent = [];
  const handshakes = [];
  const timers = [];
  const listeners = {};
  const documentListeners = {};
  let hostListener = null;
  class Base {}
  class MessageEvent extends Base { get data() { return this._data; } }
  class Event extends Base {
    get target() { return this._target; }
    get defaultPrevented() { return Boolean(this._prevented); }
    preventDefault() { this._prevented = true; }
  }
  class Selection { toString() { return this._text; } }
  class Element { closest() { return null; } }
  class HTMLAnchorElement extends Element { get href() { return this._href; } }
  class MessageChannel {
    constructor() {
      this.port1 = { postMessage: message => sent.push(message), start() {},
        addEventListener: (name, callback) => { hostListener = callback; } };
      this.port2 = { transferred: true };
    }
  }
  const head = node('HEAD');
  const body = node('BODY', {
    innerHTML: `<p>${bodyText}</p>${scriptText}`,
    cloneNode: () => ({ querySelectorAll: () => ({ forEach() {} }), textContent: bodyText }),
  });
  const markerNode = { nodeType: 8, data: marker, remove() { this.removed = true; } };
  const bootstrap = { remove() { this.removed = true; } };
  const document = {
    currentScript: bootstrap, firstChild: markerNode, head, body,
    documentElement: node('HTML'),
    createElement: tag => node(tag.toUpperCase()),
    getElementById: id => head.children.find(child => child.id === id) || null,
    addEventListener(name, callback) { (documentListeners[name] ||= []).push(callback); },
  };
  const window = {
    parent: { postMessage: (message, origin, ports) => handshakes.push({ message, origin, ports }) },
    getSelection: () => null,
    addEventListener(name, callback) { (listeners[name] ||= []).push(callback); },
  };
  const context = vm.createContext({
    window, document, MessageChannel, MessageEvent, Event, Selection, Element, HTMLAnchorElement, Reflect,
    performance: { now: () => 0 }, getComputedStyle: () => ({ opacity: '1' }), Number, String, Boolean,
    setTimeout: (callback, delay) => { timers.push({ callback, delay }); return timers.length; },
    clearTimeout: id => { if (timers[id - 1]) timers[id - 1].cleared = true; },
  });
  vm.runInContext(bridge, context);
  const host = data => { const event = new MessageEvent(); event._data = data; hostListener(event); };
  return {
    window, document, sent, handshakes, markerNode, bootstrap, head, listeners,
    command: (type, extra = {}) => host({ type, card_id: '42', ...extra }),
    host,
    runTimers() {
      for (let guard = 0; guard < 10; guard++) {
        const due = timers.filter(timer => !timer.cleared && !timer.fired);
        if (!due.length) return;
        for (const timer of due) { timer.fired = true; timer.callback(); }
      }
    },
    timers,
    fitRequests: () => sent.filter(message => message.type === 'fit_request'),
    diagnostics: phase => sent.filter(message => message.type === 'card_diagnostic' && message.phase === phase),
  };
}

test('the bridge hands its private port to the parent before authored script runs', () => {
  const h = harness();
  assert.equal(h.handshakes.length, 1);
  const { message, origin, ports } = h.handshakes[0];
  assert.deepEqual({ ...message }, { type: 'card_diagnostic', phase: 'bridge_ready', card_id: '42', document_revision: 3 });
  assert.equal(origin, '*');
  assert.equal(ports[0].transferred, true);
  assert.equal(h.markerNode.removed, true, 'the identity comment is removed');
  assert.equal(h.bootstrap.removed, true, 'the bootstrap removes itself');
  assert.equal(h.head.children[0].dataset.sgtCardIdentity, '42:3');
  const ready = h.diagnostics('bridge_ready');
  assert.equal(ready.length, 1);
  assert.equal(ready[0].text_len, 'result'.length);
  assert.equal(h.document.documentElement.style['user-select'], 'text');
});

test('fits wait for font activation, then run once', () => {
  const h = harness();
  h.window.__SGT_REQUEST_FIT__(true);
  h.runTimers();
  assert.equal(h.fitRequests().length, 0, 'no fit before the font is active');
  h.command('activate_font');
  assert.ok(h.document.documentElement.classList.names.has('sgt-font-ready'));
  h.runTimers();
  assert.deepEqual(h.fitRequests().map(message => message.streaming), [true]);
  assert.equal(h.diagnostics('activate_font_received').length, 1);
});

test('a window resize asks the parent for a fresh fit', () => {
  const h = harness();
  h.command('activate_font');
  h.runTimers();
  const before = h.fitRequests().length;
  for (const listener of h.listeners.resize) listener();
  assert.equal(h.timers.at(-1).delay, 40);
  h.runTimers();
  assert.equal(h.fitRequests().length, before + 1);
});

test('run_fit relays animation frames through the parent and reports completion', () => {
  const h = harness();
  const ticks = [];
  h.window.__SGT_RUN_FIT__ = streaming => {
    const context = h.window.__SGT_FIT_CONTEXT__;
    assert.equal(streaming, false);
    assert.equal(context.fontReady, true);
    assert.equal(context.settleBeforeReveal, true);
    context.scheduleFrame(time => ticks.push(time));
    const cancelled = context.scheduleFrame(() => ticks.push('cancelled'));
    context.cancelFrame(cancelled);
    context.complete();
  };
  h.command('run_fit', { streaming: false, settle_before_reveal: true });
  assert.equal(h.window.__SGT_FIT_CONTEXT__, null, 'the fit context does not outlive the fit');
  const frames = h.sent.filter(message => message.type === 'frame_request');
  assert.equal(frames.length, 2);
  assert.ok(h.sent.some(message => message.type === 'fit_complete'));
  for (const frame of frames) h.command('frame_tick', { handle: frame.handle, timestamp: 99 });
  assert.deepEqual(ticks, [99]);
});

test('a document without the fitter completes run_fit immediately', () => {
  const h = harness();
  h.command('run_fit', { streaming: true });
  assert.ok(h.sent.some(message => message.type === 'fit_complete'));
});

test('commands for another card or outside the bridge vocabulary have no effect', () => {
  const h = harness();
  h.host({ type: 'activate_font', card_id: '7' });
  assert.equal(h.document.documentElement.classList.names.size, 0);
  assert.equal(h.diagnostics('command_rejected').length, 1);
  const before = h.sent.length;
  for (const type of ['stream_update', 'finalize', 'set_html']) h.command(type, { html: '<p>x</p>' });
  assert.equal(h.sent.length, before, 'content is never pushed into an isolated document');
  assert.equal(h.document.body.innerHTML, '<p>result</p>');
});

test('theme updates install the stylesheet once and replace its text', () => {
  const h = harness();
  h.command('theme_update', { css: 'body{color:red}' });
  h.command('theme_update', { css: 'body{color:blue}' });
  const styles = h.document.head.children.filter(child => child.id === 'sgt-theme-css');
  assert.equal(styles.length, 1);
  assert.equal(styles[0].textContent, 'body{color:blue}');
});

test('authored script errors are reported as diagnostics', () => {
  const h = harness();
  for (const listener of h.listeners.error) listener({ error: new Error('boom') });
  const reported = h.diagnostics('script_error');
  assert.equal(reported.length, 1);
  assert.equal(reported[0].error, 'boom');
});

test('frame requests are rate limited at the source', () => {
  const h = harness();
  h.window.__SGT_RUN_FIT__ = () => {
    for (let index = 0; index < 200; index++) h.window.__SGT_FIT_CONTEXT__.scheduleFrame(() => {});
  };
  h.command('run_fit', { streaming: true });
  assert.equal(h.sent.filter(message => message.type === 'frame_request').length, 120);
});
