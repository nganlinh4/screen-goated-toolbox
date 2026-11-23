const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { createRequire } = require('node:module');
const frontendRequire = createRequire(require.resolve('../../../../../screen-record/package.json'));
const { JSDOM } = frontendRequire('jsdom');

function harness(t) {
  const dom = new JSDOM('<div id="button-container"></div><div id="content"></div>', {
    runScripts: 'dangerously', pretendToBeVisual: true,
  });
  t.after(() => dom.window.close());
  const { window } = dom;
  const frames = [], messages = [];
  window.requestAnimationFrame = callback => frames.push(callback);
  window.ipc = {postMessage: message => messages.push(JSON.parse(message))};
  const read = name => fs.readFileSync(path.join(__dirname, name), 'utf8');
  const rustScript = name => read(name).split('r#"')[1].split('"#')[0];
  window.eval(rustScript('runtime.rs').replace('#L10N_JSON#', '{}').replace('#ICON_SVGS_JSON#', '{}'));
  window.eval(read('localization_runtime.js'));
  window.eval(rustScript('refine_runtime.rs'));
  window.eval(read('../host_command_runtime.js'));
  return {window, document: window.document, frames, messages};
}

function locale(prefix) {
  return Object.fromEntries(['back', 'forward', 'opacity', 'copy', 'undo', 'redo',
    'edit', 'copy_image', 'download', 'speaker', 'result_handle', 'cancel',
    'overlay_refine_placeholder'].map(key => [key, `${prefix} ${key} "<&>`]));
}

test('locale commands update mounted tooltips while preserving control identity and user input', t => {
  const h = harness(t), first = locale('first'), second = locale('second');
  const controls = h.document.getElementById('button-container');
  h.window.applyHostCommand({type: 'locale', strings: first});
  const group = h.document.createElement('div');
  group.innerHTML = h.window.generateButtonsHTML('42', {navDepth: 1, maxNavDepth: 2,
    hasUndo: true, hasRedo: true}, false);
  controls.appendChild(group);
  const buttons = [...group.querySelectorAll('[data-l10n-title]')];
  assert.deepEqual(buttons.map(button => button.dataset.l10nTitle).sort(),
    ['back', 'copy', 'download', 'edit', 'forward', 'opacity', 'redo', 'result_handle', 'speaker', 'undo']);
  for (const button of buttons) {
    assert.equal(button.getAttribute('aria-label'), first[button.dataset.l10nTitle]);
    assert.equal(button.hasAttribute('title'), false);
  }
  const slider = group.querySelector('input[type=range]');
  slider.value = '47'; slider.focus();
  const authored = h.document.getElementById('content');
  authored.innerHTML = '<button data-l10n-title="copy" title="authored">Body</button>';
  h.window.applyHostCommand({type: 'locale', strings: second});
  for (const button of buttons) {
    assert.equal(button.getAttribute('aria-label'), second[button.dataset.l10nTitle]);
    assert.equal(button.isConnected, true);
  }
  assert.equal(group.querySelector('input[type=range]'), slider);
  assert.equal(slider.value, '47');
  assert.equal(h.document.activeElement, slider);
  assert.equal(authored.firstChild.title, 'authored');
});

test('a composing refinement keeps its draft, selection, focus and session across locale changes', t => {
  const h = harness(t), first = locale('first'), second = locale('second');
  const group = h.document.createElement('div');
  h.document.getElementById('button-container').appendChild(group);
  h.window.applyHostCommand({type: 'locale', strings: first});
  const state = {isEditing: true, editSession: '7', inputText: 'initial'};
  h.window.__SGT_REFINE_EDITOR__.reconcile(group, '42', state);
  const input = group.querySelector('input'), cancel = group.querySelector('.refine-cancel');
  input.value = 'draft in progress'; input.setSelectionRange(2, 5); input.focus();
  input.dispatchEvent(new h.window.CompositionEvent('compositionstart'));
  const messageCount = h.messages.length, frameCount = h.frames.length;
  h.window.applyHostCommand({type: 'locale', strings: second});
  h.window.__SGT_REFINE_EDITOR__.reconcile(group, '42', state);
  assert.equal(group.querySelector('input'), input);
  assert.equal(input.placeholder, second.overlay_refine_placeholder);
  assert.equal(cancel.getAttribute('aria-label'), second.cancel);
  assert.equal(input.value, 'draft in progress');
  assert.deepEqual([input.selectionStart, input.selectionEnd], [2, 5]);
  assert.equal(h.document.activeElement, input);
  assert.equal(h.messages.length, messageCount);
  assert.equal(h.frames.length, frameCount);
  input.dispatchEvent(new h.window.CompositionEvent('compositionend'));
  assert.deepEqual([h.messages.at(-1).action, h.messages.at(-1).editSession, h.messages.at(-1).text],
    ['update_refine_draft', '7', 'draft in progress']);
});

test('locale received before controls mount also applies to future button and editor creation', t => {
  const h = harness(t), strings = locale('new');
  const controls = h.document.getElementById('button-container');
  controls.remove();
  h.window.applyHostCommand({type: 'locale', strings});
  h.document.body.appendChild(controls);
  const group = h.document.createElement('div');
  group.innerHTML = h.window.generateButtonsHTML('42', {copyImage: true, hasUndo: true, hasRedo: true}, true);
  controls.appendChild(group);
  const image = group.querySelector('[data-l10n-title=copy_image]');
  assert.equal(image.getAttribute('aria-label'), strings.copy_image);
  h.window.__SGT_REFINE_EDITOR__.reconcile(group, '42', {isEditing: true, editSession: '9', inputText: 'next'});
  assert.equal(group.querySelector('input').placeholder, strings.overlay_refine_placeholder);
  assert.equal(group.querySelector('.refine-cancel').getAttribute('aria-label'), strings.cancel);
});
