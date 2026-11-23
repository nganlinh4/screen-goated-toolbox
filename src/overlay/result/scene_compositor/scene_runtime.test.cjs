// Runs the shared result-card runtime (scene, frame, command, surface and
// settled-reveal scripts, as composed into the compositor document) against
// a small fake DOM, so card behaviour is asserted instead of source text.
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const test = require('node:test');
const assert = require('node:assert/strict');
const read = name => fs.readFileSync(path.join(__dirname, name), 'utf8');

function harness() {
  const messages = [];
  const frames = [];
  const timers = [];
  const fits = [];
  const applied = [];
  const portMessages = [];
  const windowListeners = {};
  let scene;
  const themeStyle = { textContent: '' };

  function element(tag) {
    const node = {
      tagName: tag.toUpperCase(), className: '', dataset: {}, hidden: false, children: [],
      attributes: {}, listeners: {}, parent: null, textContent: '', innerHTML: '', contentWindow: null,
      style: { setProperty(name, value) { this[name] = value; }, removeProperty(name) { delete this[name]; } },
      appendChild(child) {
        if (child.parent) child.parent.children.splice(child.parent.children.indexOf(child), 1);
        child.parent = this; this.children.push(child); return child;
      },
      prepend(child) { this.appendChild(child); this.children.unshift(this.children.pop()); },
      insertBefore(child, before) {
        this.appendChild(child);
        const index = this.children.indexOf(before);
        if (index >= 0) this.children.splice(index, 0, this.children.pop());
        return child;
      },
      remove() { if (this.parent) this.parent.children.splice(this.parent.children.indexOf(this), 1); this.parent = null; },
      setAttribute(name, value) { this.attributes[name] = String(value); },
      getAttribute(name) { return this.attributes[name] ?? null; },
      removeAttribute(name) { delete this.attributes[name]; },
      addEventListener(name, callback) { (this.listeners[name] ||= []).push(callback); },
      attachShadow() { const shadow = element('#shadow-root'); shadow.parent = this; this.shadowRoot = shadow; return shadow; },
      getBoundingClientRect: () => ({ left: 0, top: 0, right: 400, bottom: 200, width: 400, height: 200 }),
      get isConnected() { for (let current = this; current; current = current.parent) if (current === scene) return true; return false; },
    };
    return node;
  }
  scene = element('main');

  const window = {
    devicePixelRatio: 1, innerWidth: 1920, innerHeight: 1080, __SGT_TOPOLOGY_EPOCH__: '1',
    ipc: { postMessage: raw => messages.push(JSON.parse(raw)) },
    addEventListener(name, callback) { (windowListeners[name] ||= []).push(callback); },
    getSelection: () => null,
    __SGT_CREATE_RECTANGLE_GLOW__: () => ({ element: element('canvas'), setState() {}, resize() {}, destroy() {} }),
    __SGT_CREATE_DIRECT_RUNTIME__: entry => ({
      apply(options) {
        applied.push({ id: entry.card.dataset.id, ...options });
        entry.bodyElement.innerHTML = options.html;
        entry.bodyElement.textContent = options.html.replace(/<[^>]*>/g, '');
      },
      initGrids() {},
      destroy() {},
    }),
    __SGT_CARD_RESIZE__: { attach: () => ({ destroy() {} }) },
    __SGT_RUN_FIT__(streaming) {
      const context = window.__SGT_FIT_CONTEXT__;
      fits.push({ streaming, context });
    },
  };
  const context = vm.createContext({
    window, console,
    document: {
      getElementById: id => (id === 'scene' ? scene : id === 'sgt-theme-css' ? themeStyle : null),
      createElement: element,
      createRange: () => ({ selectNodeContents() {}, getBoundingClientRect: () => ({ width: 0, height: 0 }) }),
    },
    performance: { now: () => 0 },
    getComputedStyle: () => ({ display: 'block', visibility: 'visible', opacity: '1' }),
    requestAnimationFrame: callback => { frames.push(callback); return frames.length; },
    cancelAnimationFrame: () => {},
    setTimeout: (callback, delay) => { timers.push({ callback, delay }); return timers.length; },
    clearTimeout: () => {},
  });
  const runtime = [
    read('scene_runtime.js').replace('__SGT_CARD_CSS_JSON__', '""'),
    read('scene_frame_runtime.js'),
    read('scene_command_helpers.js').replaceAll('__SGT_BOX_RADIUS_PX__', '8'),
    read('surface_runtime.js'),
    read('settled_reveal_runtime.js'),
    // Function declarations are script globals; expose the command surface.
    'this.commands = { upsertCard, streamCard, finalizeCard, updateGeometry, removeCard, applyTheme, cards };',
  ].join('\n');
  vm.runInContext(runtime, context);

  const h = {
    window, scene, messages, fits, applied, portMessages, timers, themeStyle, commands: context.commands,
    entry: id => context.commands.cards.get(String(id)),
    diagnostics: phase => messages.filter(message => message.type === 'card_diagnostic' && message.phase === phase),
    flushFrames() {
      for (let guard = 0; frames.length && guard < 100; guard++) frames.shift()(16);
    },
    model(id, overrides = {}) {
      return {
        id, rect: { x: 10, y: 20, width: 400, height: 200 }, stack_order: 1, visible: true,
        opacity: 100, background: 'black', body: '<p>hello</p>', streaming: false,
        streaming_enabled: true, ...overrides,
      };
    },
    // Simulates the isolated document's bridge handshake over a MessagePort.
    connectBridge(id, revision) {
      const entry = h.entry(id);
      const port = {
        listeners: [], start() {}, close() { this.closed = true; },
        addEventListener(name, callback) { this.listeners.push(callback); },
        postMessage(message) { portMessages.push({ id, ...message }); },
      };
      for (const listener of windowListeners.message || []) {
        listener({ data: { type: 'card_diagnostic', phase: 'bridge_ready', card_id: String(id),
          document_revision: revision ?? entry.revision }, source: entry.frame.contentWindow, ports: [port] });
      }
      return port;
    },
  };
  return h;
}

test('ordinary cards share one direct surface and never create a document', () => {
  const h = harness();
  h.commands.upsertCard(h.model(1));
  const entry = h.entry(1);
  assert.equal(entry.mode, 'direct');
  assert.equal(entry.frame.isConnected, false, 'a detached iframe has no browsing context');
  assert.equal(entry.directHost.hidden, false);
  assert.ok(entry.directHost.shadowRoot, 'direct content lives in an open shadow root');
  assert.deepEqual(h.applied.map(item => item.html), ['<p>hello</p>']);
});

test('raw HTML loads into a sandboxed, referrer-free, opaque-origin frame', () => {
  const h = harness();
  const html = '<!doctype html><script>parent.secret()</script>';
  h.commands.upsertCard(h.model(1, { document: html }));
  const entry = h.entry(1);
  assert.equal(entry.mode, 'isolated');
  assert.equal(entry.frame.isConnected, true);
  assert.equal(entry.frame.hidden, false);
  assert.equal(entry.directHost.hidden, true);
  const sandbox = entry.frame.getAttribute('sandbox').split(' ');
  assert.ok(sandbox.includes('allow-scripts'));
  assert.equal(sandbox.includes('allow-same-origin'), false);
  assert.equal(entry.frame.referrerPolicy, 'no-referrer');
  assert.equal(entry.frame.srcdoc, `<!--sgt-card:1:${entry.revision}-->${html}`);
  assert.equal(h.applied.length, 0, 'raw HTML never reaches the shared direct surface');
  assert.equal(h.diagnostics('document_content_committed').length, 1);
});

test('leaving raw HTML restores the direct surface and drops the frame command port', () => {
  const h = harness();
  h.commands.upsertCard(h.model(1, { document: '<p>raw</p>' }));
  const port = h.connectBridge(1);
  h.commands.upsertCard(h.model(1, { body: '<p>plain</p>' }));
  const entry = h.entry(1);
  assert.equal(entry.mode, 'direct');
  assert.equal(entry.frame.hidden, true);
  assert.equal(port.closed, true);
  assert.equal(entry.commandPort, null);
});

test('the bridge handshake activates the document only for its own revision', () => {
  const h = harness();
  h.commands.upsertCard(h.model(1, { document: '<p>raw</p>' }));
  const entry = h.entry(1);
  const stale = entry.revision;
  h.commands.upsertCard(h.model(1, { document: '<p>next</p>' }));
  assert.ok(entry.revision > stale);

  h.connectBridge(1, stale);
  assert.equal(entry.ready, false, 'a bridge from the replaced document is ignored');
  assert.equal(entry.commandPort, null);

  h.connectBridge(1);
  assert.equal(entry.ready, true);
  assert.deepEqual(h.portMessages.slice(0, 2).map(message => message.type), ['topology_epoch', 'activate_font']);
  assert.ok(h.portMessages.every(message => message.card_id === '1'));
});

test('a finalized card ignores late stream updates', () => {
  const h = harness();
  h.commands.upsertCard(h.model(1, { streaming: true, body: '<p>part</p>' }));
  h.flushFrames();
  h.commands.finalizeCard(h.model(1, { body: '<p>final</p>' }));
  h.flushFrames();
  const before = h.applied.length;
  h.commands.streamCard(h.model(1, { body: '<p>late</p>' }));
  h.flushFrames();
  assert.equal(h.applied.length, before);
  assert.equal(h.applied.at(-1).html, '<p>final</p>');
  assert.equal(h.diagnostics('stale_stream_ignored').length, 1);
});

test('terminal content supersedes a queued stream batch without waiting a frame', () => {
  const h = harness();
  h.commands.upsertCard(h.model(1, { streaming: true, body: '<p>one</p>' }));
  h.commands.streamCard(h.model(1, { body: '<p>two</p>' }));
  h.commands.finalizeCard(h.model(1, { body: '<p>done</p>' }));
  assert.deepEqual(h.applied.map(item => [item.html, item.finalizing]), [['<p>done</p>', true]]);
  h.flushFrames();
  assert.equal(h.applied.length, 1, 'the superseded stream batch is never painted');
});

test('fits run one card at a time and coalesce updates per card', () => {
  const h = harness();
  h.commands.upsertCard(h.model(1, { streaming: true }));
  h.commands.upsertCard(h.model(2, { streaming: true }));
  h.flushFrames();
  assert.equal(h.fits.length, 1, 'a second fit waits for the active one');
  const first = h.fits[0].context;
  first.complete(false);
  h.flushFrames();
  assert.equal(h.fits.length, 2);
  assert.notEqual(h.fits[1].context.body, first.body);
});

test('a fit for superseded content is never reported current', () => {
  const h = harness();
  h.commands.upsertCard(h.model(1, { streaming: true, body: '<p>a</p>' }));
  h.flushFrames();
  const { context } = h.fits[0];
  assert.equal(context.isCurrent(), true);
  h.commands.streamCard(h.model(1, { body: '<p>a b</p>' }));
  h.flushFrames();
  assert.equal(context.isCurrent(), false);
});

test('geometry is applied in CSS pixels and hides cards that become invisible', () => {
  const h = harness();
  h.window.devicePixelRatio = 2;
  h.commands.upsertCard(h.model(1, { rect: { x: 100, y: 40, width: 800, height: 400 } }));
  const entry = h.entry(1);
  assert.equal(entry.card.style.width, '400px');
  assert.equal(entry.card.style.height, '200px');
  assert.equal(entry.card.style.transform, 'translate3d(50px,20px,0)');
  h.commands.updateGeometry({ id: 1, rect: { x: 0, y: 0, width: 800, height: 400 }, visible: false });
  assert.equal(entry.card.hidden, true);
});

test('web links in direct content leave the compositor as a navigation request', () => {
  const h = harness();
  h.commands.upsertCard(h.model(1));
  const shadow = h.entry(1).directHost.shadowRoot;
  const click = href => {
    const event = { defaultPrevented: false, preventDefault() { this.defaultPrevented = true; },
      composedPath: () => [{ tagName: 'A', href }] };
    for (const listener of shadow.listeners.click) listener(event);
    return event;
  };
  assert.equal(click('javascript:alert(1)').defaultPrevented, false);
  assert.equal(click('https://example.com/a').defaultPrevented, true);
  const requests = h.messages.filter(message => message.type === 'navigation_request');
  assert.deepEqual(requests, [{ type: 'navigation_request', id: 1, url: 'https://example.com/a', topology_epoch: '1' }]);
});

test('external navigation keeps the processing shell until the native page is ready', () => {
  const h = harness();
  const states = [];
  h.commands.upsertCard(h.model(1));
  const entry = h.entry(1);
  entry.processing.setState = value => states.push(value);
  h.commands.upsertCard(h.model(1, { navigation_loading: true }));
  assert.equal(entry.mode, 'navigation-loading');
  assert.equal(entry.card.dataset.surface, 'navigation-loading');
  assert.equal(entry.directHost.hidden, true);
  assert.equal(entry.frame.hidden, true);
  h.commands.upsertCard(h.model(1, { external_navigation: true }));
  assert.equal(entry.mode, 'native');
  assert.equal(entry.card.dataset.surface, 'native');
  assert.deepEqual(states, [true, false]);
});

test('a stale content command cannot undo the latest interaction order', () => {
  const h = harness();
  h.commands.upsertCard(h.model(1, { stack_order: 3 }));
  h.commands.upsertCard(h.model(2, { stack_order: 4 }));
  const first = h.entry(1);
  for (const listener of first.card.listeners.pointerdown) listener({});
  assert.equal(first.card.style.zIndex, '5');
  assert.equal(h.messages.at(-1).type, 'interaction');
  h.commands.upsertCard(h.model(1, { stack_order: 3, body: '<p>late</p>' }));
  assert.equal(first.card.style.zIndex, '5');
  assert.equal(first.card.style.left, undefined, 'cards move only by transform');
});

test('copying selected result text publishes it for the native clipboard', () => {
  const h = harness();
  h.commands.upsertCard(h.model(7));
  h.window.getSelection = () => ({ toString: () => 'picked' });
  const event = { preventDefault() { this.prevented = true; } };
  for (const listener of h.entry(7).directHost.shadowRoot.listeners.copy) listener(event);
  assert.equal(event.prevented, true);
  assert.deepEqual(h.messages.at(-1), { action: 'copy_selection', hwnd: '7', text: 'picked', topology_epoch: '1' });
});

test('refining and navigation loading own the processing glow, sized in CSS pixels', () => {
  const h = harness();
  h.commands.upsertCard(h.model(1));
  const entry = h.entry(1);
  const states = [];
  const sizes = [];
  entry.processing.setState = value => states.push(value);
  entry.processing.resize = (...args) => sizes.push(args);
  h.commands.upsertCard(h.model(1, { refining: true }));
  assert.equal(entry.card.dataset.processing, 'true');
  h.window.devicePixelRatio = 2;
  h.commands.updateGeometry({ id: 1, rect: { x: 0, y: 0, width: 600, height: 300 }, visible: true });
  h.commands.upsertCard(h.model(1, { rect: { x: 0, y: 0, width: 600, height: 300 } }));
  assert.equal(entry.card.dataset.processing, 'false');
  assert.deepEqual(states, [true, false]);
  assert.deepEqual(sizes.at(-1), [300, 150, 2]);
});

test('streamed updates inside one frame are applied once, with the latest content', () => {
  const h = harness();
  h.commands.upsertCard(h.model(1, { streaming: true, body: '<p>a</p>' }));
  h.flushFrames();
  const before = h.applied.length;
  for (const body of ['<p>a b</p>', '<p>a b c</p>', '<p>a b c d</p>']) {
    h.commands.streamCard(h.model(1, { body }));
  }
  assert.equal(h.applied.length, before, 'stream content waits for the frame');
  h.flushFrames();
  assert.deepEqual(h.applied.slice(before).map(item => [item.html, item.animateNewWords]),
    [['<p>a b c d</p>', true]]);
});

test('a removed card never applies its pending stream frame', () => {
  const h = harness();
  h.commands.upsertCard(h.model(1, { streaming: true, body: '<p>a</p>' }));
  h.flushFrames();
  const before = h.applied.length;
  h.commands.streamCard(h.model(1, { body: '<p>late</p>' }));
  h.commands.removeCard(1);
  h.flushFrames();
  assert.equal(h.applied.length, before);
  assert.equal(h.entry(1), undefined);
});

test('a resize asks for a fit 40 ms later; an unchanged size does not', () => {
  const h = harness();
  h.commands.upsertCard(h.model(1));
  h.flushFrames();
  h.fits.at(-1)?.context.complete(false);
  const timers = h.timers.length;
  h.commands.updateGeometry({ id: 1, rect: { x: 50, y: 20, width: 400, height: 200 }, visible: true });
  assert.equal(h.timers.length, timers, 'moving without resizing never refits');
  h.commands.updateGeometry({ id: 1, rect: { x: 50, y: 20, width: 500, height: 200 }, visible: true });
  const refit = h.timers.at(-1);
  assert.equal(refit.delay, 40);
  const fits = h.fits.length;
  refit.callback();
  h.flushFrames();
  assert.equal(h.fits.length, fits + 1);
});

test('text-only source replacements carry their backdrop, colour and source size into the runtime', () => {
  const h = harness();
  h.window.devicePixelRatio = 2;
  h.commands.upsertCard(h.model(1, {
    presentation: 'text_only', source_replacement: true, backdrop_data_url: 'data:image/png;base64,AA==',
    foreground_color: '#123456', preferred_font_size: 32, source_vertical: true,
    source_regions: [{ x: 0, y: 0, width: 10, height: 10 }], source_segments: ['x'],
  }));
  const entry = h.entry(1);
  assert.equal(entry.card.dataset.presentation, 'text_only');
  assert.equal(entry.card.dataset.sourceReplacement, 'true');
  assert.equal(entry.backdrop.src, 'data:image/png;base64,AA==');
  assert.equal(entry.backdrop.hidden, false);
  assert.equal(entry.directHost.style['--text-color'], '#123456');
  h.flushFrames();
  const applied = h.applied.at(-1);
  assert.equal(applied.sourceReplacement, true);
  assert.equal(applied.preferredFontSize, 16, 'source size is converted to CSS pixels');
  assert.equal(applied.sourceVertical, true);
  assert.deepEqual(applied.sourceSegments, ['x']);
});

test('a hidden source replacement stays mounted and prewarmed but transparent', () => {
  const h = harness();
  h.commands.upsertCard(h.model(1, { source_replacement: true, visible: false }));
  const entry = h.entry(1);
  assert.equal(entry.card.hidden, false);
  assert.equal(entry.card.style.opacity, '0');
  assert.equal(entry.card.style.pointerEvents, 'none');
  assert.equal(entry.sourceSurfacePrewarmed, true);
  h.commands.upsertCard(h.model(2, { visible: false }));
  assert.equal(h.entry(2).card.hidden, true, 'ordinary hidden cards unmount');
});

test('source replacements never run the ordinary result fitter', () => {
  const h = harness();
  h.commands.upsertCard(h.model(1, { source_replacement: true, streaming: true, body: '<p>a</p>' }));
  h.flushFrames();
  h.commands.finalizeCard(h.model(1, { source_replacement: true, body: '<p>done</p>' }));
  h.flushFrames();
  assert.equal(h.fits.length, 0);
  assert.equal(h.applied.at(-1).html, '<p>done</p>');
  h.commands.upsertCard(h.model(2, { streaming: true }));
  h.flushFrames();
  assert.equal(h.fits.length, 1, 'ordinary cards still fit');
});

test('a theme updates the shared stylesheet and each card background', () => {
  const h = harness();
  h.commands.upsertCard(h.model(1));
  h.commands.applyTheme({ css: ':root{--x:1}', cards: [{ id: 1, background: 'white' }, { id: 9, background: 'red' }] });
  assert.equal(h.themeStyle.textContent, ':root{--x:1}');
  assert.equal(h.entry(1).card.style.background, 'white');
});

test('non-streaming content stays hidden until its final fit completes', () => {
  const h = harness();
  h.commands.upsertCard(h.model(1, { streaming_enabled: false, body: '<p>whole answer</p>' }));
  const entry = h.entry(1);
  assert.equal(entry.directHost.style.visibility, 'hidden');
  h.flushFrames();
  const { context } = h.fits.at(-1);
  assert.equal(context.settleBeforeReveal, true);
  assert.equal(context.handlesRefinement, true, 'the shared fitter owns refinement');
  assert.equal(entry.directHost.style.visibility, 'hidden', 'painting alone does not reveal');
  assert.equal(h.diagnostics('final_painted').length, 0, 'the paint report waits for the reveal');
  context.complete(false);
  assert.equal(entry.directHost.style.visibility, 'visible');
  assert.equal(h.diagnostics('final_fit_completed').length, 1);
  h.flushFrames();
  assert.equal(h.diagnostics('final_painted').length, 1);
});

test('streamed content is never hidden behind the settled reveal', () => {
  const h = harness();
  h.commands.upsertCard(h.model(1, { streaming: true }));
  h.commands.finalizeCard(h.model(1, { body: '<p>done</p>' }));
  assert.notEqual(h.entry(1).directHost.style.visibility, 'hidden');
});

test('interactive raw documents are accepted only while their surface is visible', () => {
  const h = harness();
  h.commands.upsertCard(h.model(1, { document: '<button>go</button>' }));
  const port = h.connectBridge(1);
  const report = () => port.listeners.forEach(listener => listener({ data: {
    type: 'card_diagnostic', phase: 'interactive_document_alive' } }));
  report();
  assert.equal(h.diagnostics('interactive_surface_visible').length, 1);
  h.entry(1).frame.hidden = true;
  report();
  const rejected = h.diagnostics('interactive_surface_rejected');
  assert.equal(rejected.length, 1);
  assert.match(rejected[0].error, /frame_hidden=true/);
});

test('removing a card cancels a drag that targets it', () => {
  const h = harness();
  const cancelled = [];
  h.window.cancelResultDragForCard = id => cancelled.push(id);
  h.commands.upsertCard(h.model(4));
  h.commands.removeCard(4);
  assert.deepEqual(cancelled, ['4']);
});
