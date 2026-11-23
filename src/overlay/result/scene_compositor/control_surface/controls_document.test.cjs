// Runs the composed control script (in document_script order) and the control
// stylesheet in a DOM, so the rendered controls are asserted instead of text.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { createRequire } = require('node:module');
const frontendRequire = createRequire(require.resolve('../../../../../screen-record/package.json'));
const { JSDOM } = frontendRequire('jsdom');

const read = name => fs.readFileSync(path.join(__dirname, name), 'utf8');
const rustScript = name => read(name).split('r#"')[1].split('"#')[0];
const ICONS = ['arrow_back', 'arrow_forward', 'undo', 'redo', 'hourglass_empty', 'stop',
  'cleaning_services', 'content_copy', 'check', 'download', 'filter', 'volume_up', 'mic', 'send',
  'opacity', 'close'];
const icons = Object.fromEntries(ICONS.map(name => [name, `<svg data-icon="${name}"></svg>`]));
const l10n = { back: 'Back', forward: 'Forward', opacity: 'Opacity', copy: 'Copy', download: 'Download',
  speaker: 'Speak', edit: 'Edit', undo: 'Undo', redo: 'Redo', result_handle: 'Move' };

function harness(t) {
  const dom = new JSDOM(`<style>${rustScript('css.rs')}</style><aside id="button-container"></aside>`, {
    runScripts: 'dangerously', pretendToBeVisual: true,
  });
  t.after(() => dom.window.close());
  const { window } = dom;
  const messages = [];
  const frames = [];
  window.ipc = { postMessage: raw => messages.push(JSON.parse(raw)) };
  window.requestAnimationFrame = callback => frames.push(callback);
  window.ResizeObserver = class { observe() {} unobserve() {} };
  window.document.fonts = { addEventListener() {} };
  // The DOM has no layout; give the measured toolbar a size so it is placed.
  for (const [name, value] of [['offsetWidth', 240], ['offsetHeight', 28]]) {
    Object.defineProperty(window.HTMLElement.prototype, name, { get: () => value });
  }
  window.__SGT_CONTROL_GEOMETRY_CACHE__ = () => ({ merge() {}, clear() {}, get: () => null });
  // One script, as document_script() concatenates it: the parts share lexical globals.
  window.eval([
    rustScript('runtime.rs').replace('#L10N_JSON#', JSON.stringify(l10n))
      .replace('#ICON_SVGS_JSON#', JSON.stringify(icons)),
    read('localization_runtime.js'), read('tooltip_runtime.js'), read('source_anchor.js'), read('source_dock.js'),
    read('placement.js'), read('source_geometry.js'), read('processing_preview.js'),
    read('layout_runtime.js'), rustScript('drag_runtime.rs'), rustScript('refine_runtime.rs'),
  ].join(''));
  const show = (state, id = '1') => window.updateWindows({
    [id]: { rect: { x: 400, y: 300, w: 200, h: 100 }, state: { opacityPercent: 100, ...state } },
  });
  const group = (id = '1') => window.document.querySelector(`.button-group[data-hwnd="${id}"]`);
  return { window, document: window.document, messages, frames, show, group };
}

function rule(document, selector) {
  for (const sheet of document.styleSheets) {
    for (const cssRule of sheet.cssRules) if (cssRule.selectorText === selector) return cssRule.style;
  }
  return null;
}

test('toolbar tooltips follow their control, stay within the work area, and publish only a visual rect', t => {
  const h = harness(t);
  h.show({});
  const group = h.group();
  group.controlPlacement.work = { x: 0, y: 0, w: 1000, h: 760 };
  const rect = (left, top, width, height) => ({ left, top, right: left + width,
    bottom: top + height, width, height });
  const nativeBounds = h.window.HTMLElement.prototype.getBoundingClientRect;
  h.window.HTMLElement.prototype.getBoundingClientRect = function () {
    if (!this.matches('.result-control-tooltip')) return nativeBounds.call(this);
    return rect(parseFloat(this.style.left) || 0, parseFloat(this.style.top) || 0, 200, 32);
  };
  const nativeTimeout = h.window.setTimeout.bind(h.window);
  let reveal;
  h.window.setTimeout = (callback, delay) => {
    if (delay !== 450) return nativeTimeout(callback, delay);
    reveal = callback;
    return 1;
  };
  const broom = group.querySelector('.result-handle');
  const copy = group.querySelector('[data-l10n-title="copy"]');
  const lastTooltipMessage = () => h.messages.findLast(message => message.action === 'control_tooltip_region');
  broom.getBoundingClientRect = () => rect(970, 734, 24, 24);
  copy.getBoundingClientRect = () => rect(420, 320, 24, 24);
  const hover = (element, x, y) => element.firstElementChild.dispatchEvent(
    new h.window.MouseEvent('pointerover', { bubbles: true, clientX: x, clientY: y }));

  hover(broom, 980, 740);
  reveal();
  const tooltip = h.document.querySelector('.result-control-tooltip');
  assert.equal(broom.hasAttribute('title'), false);
  assert.equal(tooltip.textContent, 'Move');
  assert.equal(h.window.getComputedStyle(tooltip).pointerEvents, 'none');
  assert.deepEqual(lastTooltipMessage(), { action: 'control_tooltip_region', topology_epoch: '1',
    rect: { x: 794, y: 696, width: 200, height: 32 } });

  hover(copy, 430, 330);
  reveal();
  assert.equal(tooltip.textContent, 'Copy');
  assert.deepEqual(lastTooltipMessage().rect, { x: 332, y: 350, width: 200, height: 32 });
  h.window.__SGT_SET_CONTROL_LOCALE__({ ...l10n, copy: 'Sao chép' });
  assert.equal(tooltip.textContent, 'Sao chép');

  // Win32 can stop forwarding events when the cursor leaves the input region.
  h.window.updateCursorPosition(800, 100);
  assert.equal(tooltip.hidden, true);
  assert.deepEqual(lastTooltipMessage(), { action: 'control_tooltip_region', topology_epoch: '1', rect: null });
});

test('the model badge renders as escaped text immediately before the opacity control', t => {
  const h = harness(t);
  h.show({ modelLabel: '<b>Model</b> & co' });
  const badge = h.group().querySelector('.model-badge');
  assert.equal(badge.textContent, '<b>Model</b> & co');
  assert.equal(badge.querySelector('b'), null);
  assert.equal(badge.classList.contains('btn'), false, 'the badge is not a control');
  assert.ok(badge.nextElementSibling.classList.contains('opacity-btn-expandable'));
  h.show({ modelLabel: 'Model', isBrowsing: true });
  assert.equal(h.group().querySelector('.model-badge'), null);
  h.show({ modelLabel: 'Model', groupActions: true });
  assert.equal(h.group().querySelector('.model-badge'), null);
});

test('the model badge is a readable, non-interactive, never-clipped surface', t => {
  const h = harness(t);
  const badge = rule(h.document, '.model-badge');
  assert.equal(badge.getPropertyValue('cursor'), 'default');
  assert.equal(badge.getPropertyValue('pointer-events'), 'none');
  assert.equal(badge.getPropertyValue('background'), 'var(--btn-bg)');
  assert.equal(badge.getPropertyValue('text-overflow'), '');
  assert.equal(badge.getPropertyValue('max-width'), '');
  assert.notEqual(badge.getPropertyValue('overflow'), 'hidden');
  assert.equal(rule(h.document, '.model-badge:hover'), null);
});

test('control sizes follow --control-scale and the scale comes from the card state', t => {
  const h = harness(t);
  assert.equal(rule(h.document, '.button-group').getPropertyValue('--control-scale').trim(), '1');
  assert.equal(rule(h.document, '.btn').getPropertyValue('width'), 'calc(24px * var(--control-scale))');
  h.show({ controlScalePercent: 150 });
  assert.equal(h.group().style.getPropertyValue('--control-scale'), '1.5');
  h.show({ controlScalePercent: 1000 });
  assert.equal(h.group().style.getPropertyValue('--control-scale'), '3', 'scale is clamped');
});

test('opacity echoes keep the slider element and only move its value', t => {
  const h = harness(t);
  h.show({ opacityPercent: 80 });
  const slider = h.group().querySelector('.opacity-slider-inline');
  const copy = h.group().querySelector('[data-l10n-title="copy"]');
  h.show({ opacityPercent: 55 });
  assert.equal(h.group().querySelector('.opacity-slider-inline'), slider);
  assert.equal(h.group().querySelector('[data-l10n-title="copy"]'), copy, 'buttons are not rebuilt');
  assert.equal(slider.value, '55');
  assert.equal(h.group().querySelector('.opacity-value-inline').textContent, '55%');
});

test('copy, download and speaker post their action for the owning card', t => {
  const h = harness(t);
  h.show({}, '42');
  for (const name of ['copy', 'download', 'speaker']) {
    h.group('42').querySelector(`[data-l10n-title="${name}"]`).click();
    assert.deepEqual(h.messages.at(-1), { action: name, hwnd: '42', topology_epoch: '1' });
  }
  const actions = [...h.group('42').querySelectorAll('[onclick]')].map(node => node.getAttribute('onclick'));
  assert.equal(actions.some(text => /dismiss_chain|copy_all/.test(text)), false);
});

test('the speaker button reflects loading and speaking state', t => {
  const h = harness(t);
  h.show({ ttsLoading: true });
  assert.ok(h.group().querySelector('[data-l10n-title="speaker"] [data-icon="hourglass_empty"]'));
  h.show({ ttsSpeaking: true });
  assert.ok(h.group().querySelector('[data-l10n-title="speaker"] [data-icon="stop"]'));
  h.show({});
  assert.ok(h.group().querySelector('[data-l10n-title="speaker"] [data-icon="volume_up"]'));
});

test('a control color picks the contrasting local surface', t => {
  const h = harness(t);
  h.show({ controlColor: '#101010' });
  assert.ok(h.group().classList.contains('local-control-surface-light'));
  assert.equal(h.group().style.getPropertyValue('--chain-control-color'), '#101010');
  h.show({ controlColor: '#f0f0f0' });
  assert.ok(h.group().classList.contains('local-control-surface-dark'));
  assert.ok(!h.group().classList.contains('local-control-surface-light'));
  h.show({});
  assert.ok(!h.group().classList.contains('local-control-surface-dark'));
  assert.equal(h.group().style.getPropertyValue('--chain-control-color'), '');
  for (const surface of ['light', 'dark']) {
    assert.ok(rule(h.document, `.button-group.local-control-surface-${surface}`));
  }
});

test('an open refinement editor replaces the buttons and pins the controls visible', t => {
  const h = harness(t);
  h.show({ isEditing: true, editSession: '3', inputText: 'draft' });
  const group = h.group();
  assert.ok(group.classList.contains('proximity-pinned'));
  assert.equal(group.querySelector('.opacity-btn-expandable'), null);
  assert.equal(group.querySelector('input').value, 'draft');
  h.show({});
  assert.ok(!h.group().classList.contains('proximity-pinned'));
  assert.ok(h.group().querySelector('.opacity-btn-expandable'));
});

test('proximity fades controls over 150 px and invalidation republishes unchanged regions', t => {
  const h = harness(t);
  h.show({});
  const group = h.group();
  group.getBoundingClientRect = () => ({ left: 100, top: 100, right: 200, bottom: 130, width: 100, height: 30 });
  h.window.updateCursorPosition(275, 115);
  assert.equal(Number(group.style.opacity), 0.5);
  const published = h.messages.filter(m => m.action === 'update_clickable_regions').length;
  h.window.updateCursorPosition(275, 115);
  assert.equal(h.messages.filter(m => m.action === 'update_clickable_regions').length, published,
    'an unchanged region is not republished');
  h.window.invalidateButtonRegions();
  h.window.updateCursorPosition(275, 115);
  assert.equal(h.messages.filter(m => m.action === 'update_clickable_regions').length, published + 1);
  h.window.restoreButtonRegionsAfterDrag(9);
  assert.equal(h.messages.at(-1).action, 'restore_clickable_regions');
  assert.equal(h.messages.at(-1).gesture_id, 9);
});

test('a middle-button drag moves every card; a group drag moves its group', t => {
  const h = harness(t);
  h.window.__SGT_BUTTON_SCENE__ = { allocateGestureId: () => 5, setDragActive() {} };
  h.window.registeredWindows = { 1: { state: { groupIds: [1, 2] } }, 2: { state: { groupIds: [1, 2] } },
    3: { state: {} } };
  const event = button => ({ button, pointerId: 1, clientX: 0, clientY: 0, pointerType: 'mouse',
    preventDefault() {}, currentTarget: { setPointerCapture() {} } });
  h.window.handleResultDrag(event(1), '1', false);
  assert.equal(h.messages.at(-1).action, 'result_all_drag_start');
  assert.equal(h.messages.at(-1).gesture_id, 5);
  assert.ok(h.window.shouldPreserveResultDragGeometry('3'));
  h.window.resetResultDragTopology();
  h.window.handleResultDrag(event(0), '1', true);
  assert.equal(h.messages.at(-1).action, 'result_group_drag_start');
  assert.ok(h.window.shouldPreserveResultDragGeometry('2'));
  assert.ok(!h.window.shouldPreserveResultDragGeometry('3'));
});
