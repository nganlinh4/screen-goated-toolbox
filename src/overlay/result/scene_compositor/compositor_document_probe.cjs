// Boots the composed compositor document (read from stdin) in a DOM and drives
// it through host commands and pointer input. Run by card_document.rs tests;
// exits non-zero with the failing assertion.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { createRequire } = require('node:module');
const frontendRequire = createRequire(path.join(__dirname, '../../../../screen-record/package.json'));
const { JSDOM, VirtualConsole } = frontendRequire('jsdom');

function boot(html) {
  const errors = [];
  const messages = [];
  const virtualConsole = new VirtualConsole();
  virtualConsole.on('jsdomError', error => errors.push(String(error.message || error)));
  const dom = new JSDOM(html, {
    runScripts: 'dangerously', pretendToBeVisual: true, virtualConsole,
    beforeParse(window) {
      window.ipc = { postMessage: raw => messages.push(raw.startsWith('{') ? JSON.parse(raw) : raw) };
      window.matchMedia = () => ({ matches: false, addEventListener() {}, removeEventListener() {} });
      window.ResizeObserver = class { observe() {} unobserve() {} disconnect() {} };
      window.Range.prototype.getBoundingClientRect = () => ({ left: 0, top: 0, right: 0, bottom: 0, width: 0, height: 0 });
      window.HTMLElement.prototype.setPointerCapture = function() {};
      window.document.fonts = { load: () => Promise.resolve(['face']), check: () => true, addEventListener() {} };
      // The DOM has no layout: give controls a size so they are placed.
      for (const [name, value] of [['offsetWidth', 240], ['offsetHeight', 28]]) {
        Object.defineProperty(window.HTMLElement.prototype, name, { configurable: true, get: () => value });
      }
    },
  });
  return { dom, window: dom.window, document: dom.window.document, errors, messages };
}

function cssRules(document) {
  const rules = [];
  const visit = list => {
    for (const rule of list) {
      if (rule.cssRules) visit(rule.cssRules);
      if (rule.style) rules.push(rule);
    }
  };
  for (const sheet of document.styleSheets) visit(sheet.cssRules);
  return rules;
}

function rule(document, selector) {
  const found = cssRules(document).find(candidate => candidate.selectorText === selector);
  assert.ok(found, `missing rule ${selector}`);
  return found.style;
}

function card(id, overrides = {}) {
  return {
    id, rect: { x: 10, y: 20, width: 400, height: 200 }, control_rect: { x: 6, y: 16, width: 408, height: 208 },
    stack_order: 1, visible: true, opacity: 90, background: '#ffffff', body: '<p>hello world</p>',
    streaming: false, streaming_enabled: true, controls: { opacityPercent: 90, modelLabel: 'Model' },
    ...overrides,
  };
}

const settle = () => new Promise(resolve => setTimeout(resolve, 30));

const scenarios = {
  async boots_with_one_font_and_shared_runtimes(h) {
    await settle();
    assert.deepEqual(h.errors, []);
    assert.equal(h.messages[0].type, 'font_ready');
    assert.equal(h.messages[1], 'renderer_ready');
    for (const name of ['__SGT_CREATE_DIRECT_RUNTIME__', '__SGT_PATCH_BODY__', '__SGT_SAFE_FRAGMENT__',
      '__SGT_CREATE_REVEAL_RUNTIME__', '__SGT_RUN_FIT__', '__SGT_SHAPE_LAYOUT__', '__SGT_CARD_RESIZE__',
      '__SGT_TYPOGRAPHY_RESIZE__', '__SGT_BUTTON_SCENE__', 'updateWindows', 'applyHostCommand', 'gridjs']) {
      assert.ok(h.window[name], `${name} is installed`);
    }
    const faces = cssRules(h.document).filter(candidate => candidate.constructor.name === 'CSSFontFaceRule'
      || /^@font-face/.test(candidate.cssText));
    assert.equal(faces.length, 1, 'exactly one bundled font face');
    assert.match(faces[0].cssText, /Google Sans Flex/);
    // The CSS object model drops font `src`; read the face from its stylesheet text.
    const faceText = [...h.document.querySelectorAll('style')].map(style => style.textContent).join('\n')
      .match(/@font-face\s*\{[^}]*\}/g);
    assert.equal(faceText.length, 1);
    assert.match(faceText[0], /src:url\('font\.woff2'\) format\('woff2'\)/);
    const families = cssRules(h.document).map(candidate => candidate.style.getPropertyValue('font-family'))
      .filter(Boolean);
    assert.ok(families.every(family => !/Segoe UI/.test(family)), families.join(' | '));
    assert.equal(h.document.querySelectorAll('script[src], link[href^="http"]').length, 0, 'nothing loads remotely');
  },

  async card_chrome_is_owned_by_the_document_stylesheet(h) {
    const card = rule(h.document, '.result-card');
    assert.equal(card.getPropertyValue('border-radius'), 'var(--sgt-box-radius,8px)');
    assert.equal(card.getPropertyValue('box-shadow'), 'inset 0 0 0 1px var(--result-outline)');
    assert.equal(card.getPropertyValue('user-select'), 'text');
    const frame = rule(h.document, '.result-frame');
    assert.equal(frame.getPropertyValue('border-radius'), 'inherit');
    assert.match(frame.getPropertyValue('clip-path'), /^inset\(0 round var\(--sgt-box-radius,8px\)\)$/);
    assert.equal(rule(h.document, '.resize-handle').getPropertyValue('z-index'), '4');
    assert.equal(rule(h.document, '.result-card[data-source-replacement="true"] .resize-handle')
      .getPropertyValue('display'), 'none');
    const hidden = rule(h.document, '.direct-host[hidden],.result-frame[hidden]');
    assert.equal(hidden.getPropertyValue('display'), 'none');
    assert.equal(hidden.getPropertyPriority('display'), 'important');
    const textOnly = rule(h.document, '.result-card[data-presentation="text_only"]');
    assert.equal(textOnly.getPropertyValue('background'), 'transparent');
    assert.equal(textOnly.getPropertyValue('box-shadow'), 'none');
    assert.equal(textOnly.getPropertyValue('border-radius'), '3px');
    assert.equal(textOnly.getPropertyValue('user-select'), 'text');
    const textSurface = rule(h.document, '.result-card[data-presentation="text_only"] .direct-host,'
      + '.result-card[data-presentation="text_only"] .result-frame');
    assert.equal(textSurface.getPropertyValue('cursor'), 'text');
    const surfaces = rule(h.document, '.direct-host,.result-frame');
    assert.equal(surfaces.getPropertyValue('position'), 'absolute');
    assert.equal(surfaces.getPropertyValue('user-select'), 'text');
    assert.equal(rule(h.document, '.result-card[data-surface="native"]').getPropertyValue('box-shadow'), 'none');
  },

  async card_content_renders_in_a_scoped_shadow_surface(h) {
    await settle();
    h.window.applyHostCommand({ type: 'upsert', card: card(1) });
    await settle();
    const element = h.document.querySelector('#scene .result-card[data-id="1"]');
    assert.ok(element);
    assert.equal(element.style.width, '400px');
    assert.equal(element.style.getPropertyValue('--sgt-box-radius'), '8px');
    const shadow = element.querySelector('.direct-host').shadowRoot;
    assert.equal(shadow.querySelector('.result-body').textContent.trim(), 'hello world');
    const scopedText = [...shadow.querySelectorAll('style')].map(style => style.textContent).join('\n');
    assert.match(scopedText, /\.result-body/);
    assert.match(scopedText, /:host/);
    assert.ok(cssRules(h.document).every(candidate => !/\.result-body/.test(candidate.selectorText || '')),
      'card styles never leak into the document');
    assert.equal(element.querySelectorAll('.resize-handle').length, 8);
    assert.ok(h.messages.some(message => message.phase === 'final_fit_completed'));
  },

  async controls_share_the_document_and_follow_card_commands(h) {
    await settle();
    h.window.applyHostCommand({ type: 'upsert', card: card(1) });
    await settle();
    const group = h.document.querySelector('#button-container .button-group[data-hwnd="1"]');
    assert.ok(group, 'controls render in the compositor document');
    assert.ok(group.querySelector('.model-badge').nextElementSibling.classList.contains('opacity-btn-expandable'));
    h.window.applyHostCommand({ type: 'opacity', id: 1, opacity: 40 });
    assert.equal(h.document.querySelector('.result-card[data-id="1"]').style.opacity, '0.4');
    h.window.applyHostCommand({ type: 'theme', theme: { css: ':root{--result-outline:red}', controls_css: '.x{}', cards: [] } });
    assert.equal(h.document.getElementById('sgt-theme-css').textContent, ':root{--result-outline:red}');
    assert.equal(h.document.getElementById('sgt-controls-theme-css').textContent, '.x{}');
    h.window.applyHostCommand({ type: 'upsert', card: card(2, { stack_order: 2 }) });
    h.window.applyHostCommand({ type: 'raise', id: 1, stack_order: 5 });
    assert.equal(h.document.querySelector('.result-card[data-id="1"]').style.zIndex, '5');
    h.document.querySelector('.result-card[data-id="2"]')
      .dispatchEvent(new h.window.Event('pointerdown', { bubbles: true }));
    assert.equal(h.messages.at(-1).type, 'interaction');
    h.window.applyHostCommand({ type: 'remove', id: 2 });
    await settle();
    assert.equal(h.document.querySelector('.result-card[data-id="2"]'), null);
    assert.equal(h.document.querySelector('.button-group[data-hwnd="2"]'), null);
  },

  async a_control_drag_hides_controls_until_its_settlement(h) {
    await settle();
    h.window.applyHostCommand({ type: 'upsert', card: card(1) });
    await settle();
    const container = h.document.getElementById('button-container');
    const handle = container.querySelector('.result-handle');
    const pointer = (type, x, y) => Object.assign(new h.window.Event(type, { bubbles: true, cancelable: true }),
      { button: 0, pointerId: 1, pointerType: 'mouse', clientX: x, clientY: y });
    handle.dispatchEvent(pointer('pointerdown', 100, 100));
    const start = h.messages.find(message => message.action === 'result_drag_start');
    assert.ok(start, 'the handle starts a native drag');
    assert.equal(container.style.visibility, 'hidden');
    h.document.dispatchEvent(pointer('pointermove', 150, 130));
    h.document.dispatchEvent(pointer('pointerup', 150, 130));
    const finish = h.messages.find(message => message.action === 'result_drag_finish');
    assert.deepEqual([finish.dx, finish.dy, finish.cancelled, finish.gesture_id], [50, 30, false, start.gesture_id]);
    assert.equal(container.style.visibility, 'hidden', 'controls wait for settlement');
    h.window.applyHostCommand({ type: 'drag_settled', gesture_id: start.gesture_id,
      cards: [{ id: 1, visible: true, rect: { x: 60, y: 50, width: 400, height: 200 } }] });
    assert.equal(container.style.visibility, '');
    assert.equal(h.document.querySelector('.result-card[data-id="1"]').style.transform, 'translate3d(60px,50px,0)');
    assert.equal(h.messages.at(-1).action, 'restore_clickable_regions');
  },

  async an_edge_resize_starts_a_native_resize_gesture(h) {
    await settle();
    h.window.applyHostCommand({ type: 'upsert', card: card(1) });
    await settle();
    const edge = h.document.querySelector('.result-card[data-id="1"] .resize-handle[data-edge="se"]');
    edge.dispatchEvent(Object.assign(new h.window.Event('pointerdown', { bubbles: true, cancelable: true }),
      { button: 0, pointerId: 2, pointerType: 'mouse', clientX: 410, clientY: 220 }));
    const start = h.messages.find(message => message.action === 'result_resize_start');
    assert.deepEqual([start.hwnd, start.edge], ['1', 'se']);
    assert.equal(h.document.getElementById('button-container').style.visibility, 'hidden');
  },
};

async function main() {
  const html = fs.readFileSync(0, 'utf8');
  const selected = process.argv.slice(2);
  const names = selected.length ? selected : Object.keys(scenarios);
  for (const name of names) {
    assert.ok(scenarios[name], `unknown scenario ${name}`);
    const h = boot(html);
    try {
      await scenarios[name](h);
      assert.deepEqual(h.errors, [], `${name}: uncaught document errors`);
      console.log(`ok ${name}`);
    } finally {
      h.window.close();
    }
  }
}

main().catch(error => {
  console.error(error && error.stack ? error.stack : error);
  process.exit(1);
});
