const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const test = require('node:test');
const assert = require('node:assert/strict');

function harness() {
  const messages = [];
  const frames = [];
  function element() {
    return {
      dataset: {}, children: [], events: {}, isConnected: false,
      appendChild(child) { this.children.push(child); },
      addEventListener(name, handler) { this.events[name] = handler; },
      setSelectionRange(start, end) { this.selectionStart = start; this.selectionEnd = end; },
      setRangeText(text, start, end) {
        this.value = this.value.slice(0, start) + text + this.value.slice(end);
        this.setSelectionRange(start + text.length, start + text.length);
      },
      focus() {},
    };
  }
  const group = element();
  group.replaceChildren = function(child) {
    for (const old of this.children) { old.isConnected = false; old.children.forEach(c => c.isConnected = false); }
    this.children = [child]; child.parentElement = this; child.isConnected = true;
    child.children.forEach(c => c.isConnected = true);
  };
  const window = {L10N: {}, iconSvgs: {}, ipc: {postMessage(text) { messages.push(JSON.parse(text)); }}};
  const source = fs.readFileSync(path.join(__dirname, 'refine_runtime.rs'), 'utf8')
    .split('r#"')[1].split('"#')[0];
  vm.runInNewContext(source, {window, document: {createElement: element},
    requestAnimationFrame: callback => frames.push(callback)});
  return {window, group, messages, frames};
}

test('a reopened editor rejects text from an older session', () => {
  const h = harness();
  const api = h.window.__SGT_REFINE_EDITOR__;
  assert.equal(api.reconcile(h.group, '42', {isEditing: true, editSession: 'old', inputText: 'old'}), true);
  const oldInput = h.group.children[0].children[0];
  assert.equal(api.reconcile(h.group, '42', {isEditing: true, editSession: 'new', inputText: 'new'}), true);
  const input = h.group.children[0].children[0];
  assert.notEqual(oldInput, input);
  input.setSelectionRange(1, 2);
  h.window.setRefineText('42', 'bad', true, 'old');
  h.window.setRefineText('42', 'bad', true, null);
  assert.equal(input.value, 'new');
  h.window.setRefineText('42', 'X', true, 'new');
  assert.equal(input.value, 'nXw');
  assert.deepEqual(h.messages.map(m => [m.action, m.editSession]), [
    ['update_refine_draft', 'new'], ['request_refine_focus', 'new'],
  ]);
  oldInput.events.input({isComposing: false});
  h.frames[0]();
  assert.deepEqual(h.messages.slice(-1).map(m => [m.action, m.editSession]), [
    ['update_refine_draft', 'old'],
  ]);
});

test('editor actions carry their session and closed editors reject text', () => {
  const h = harness();
  const api = h.window.__SGT_REFINE_EDITOR__;
  assert.equal(api.reconcile(h.group, '42', {isEditing: true, editSession: null}), false);
  assert.equal(api.reconcile(h.group, '42', {isEditing: true, editSession: '7', inputText: 'hello'}), true);
  const [input, mic, send, cancel] = h.group.children[0].children;
  input.events.keydown({key: 'ArrowUp', preventDefault() {}});
  mic.events.click(); send.events.click(); cancel.events.click();
  assert.deepEqual(h.messages.map(m => [m.action, m.editSession]), [
    ['history_up_refine', '7'], ['mic', '7'], ['submit_refine', '7'], ['cancel_refine', '7'],
  ]);
  assert.equal(api.reconcile(h.group, '42', {isEditing: false, editSession: null}), false);
  h.window.setRefineText('42', 'bad', false, '7');
  assert.equal(input.value, 'hello');
});

test('rebase republishes the draft and drops a focus request from the old epoch', () => {
  const h = harness();
  h.window.__SGT_TOPOLOGY_EPOCH__ = '1';
  const api = h.window.__SGT_REFINE_EDITOR__;
  api.reconcile(h.group, '42', {isEditing: true, editSession: '7', inputText: 'old'});
  const input = h.group.children[0].children[0];
  input.value = 'new draft';
  h.window.__SGT_TOPOLOGY_EPOCH__ = '2';
  api.republishDrafts();
  h.frames[0]();
  assert.deepEqual(h.messages.map(m => [m.action, m.text, m.editSession, m.topology_epoch]), [
    ['update_refine_draft', 'new draft', '7', '2'],
  ]);
  assert.equal(input.value, 'new draft');
});

test('closing an editor releases focus only after the final editor is removed', () => {
  const h = harness();
  const api = h.window.__SGT_REFINE_EDITOR__;
  const second = { ...h.group, children: [] };
  api.reconcile(h.group, '42', {isEditing: true, editSession: '1', inputText: ''});
  api.reconcile(second, '43', {isEditing: true, editSession: '2', inputText: ''});
  h.frames[0](); h.frames[1]();
  api.remove('43'); api.settleFocusMode();
  assert.equal(api.activeCount(), 1);
  assert.equal(h.messages.filter(m => m.action === 'release_refine_focus').length, 0);
  assert.deepEqual([h.messages.at(-1).action, h.messages.at(-1).hwnd], ['request_refine_focus', '42']);
  api.remove('42'); api.settleFocusMode();
  assert.equal(api.activeCount(), 0);
  assert.equal(h.messages.filter(m => m.action === 'release_refine_focus').length, 1);
});

test('a removed editor cannot request focus on a later animation frame', () => {
  const h = harness();
  const api = h.window.__SGT_REFINE_EDITOR__;
  api.reconcile(h.group, '42', {isEditing: true, editSession: '1', inputText: ''});
  api.remove('42'); api.settleFocusMode();
  h.frames[0]();
  assert.equal(h.messages.some(m => m.action === 'request_refine_focus'), false);
  api.republishDrafts();
  assert.equal(h.messages.at(-1).action, 'release_refine_focus');
});

test('a disconnected editor cannot keep the native surface activatable', () => {
  const h = harness();
  const api = h.window.__SGT_REFINE_EDITOR__;
  api.reconcile(h.group, '42', {isEditing: true, editSession: '1', inputText: ''});
  h.frames[0]();
  h.group.children[0].children[0].isConnected = false;
  api.settleFocusMode();
  assert.equal(api.activeCount(), 0);
  assert.equal(h.messages.at(-1).action, 'release_refine_focus');
});

test('rebase retries focus only for the current editor and epoch', () => {
  const h = harness();
  h.window.__SGT_TOPOLOGY_EPOCH__ = '2';
  const api = h.window.__SGT_REFINE_EDITOR__;
  api.reconcile(h.group, '42', {isEditing: true, editSession: '1', inputText: ''});
  h.frames[0]();
  api.republishFocus();
  assert.deepEqual(h.messages.map(m => [m.action, m.editSession, m.topology_epoch]), [
    ['request_refine_focus', '1', '2'], ['request_refine_focus', '1', '2']
  ]);
  api.remove('42');
  api.republishFocus();
  assert.equal(h.messages.length, 2);
});

test('a delayed native grant cannot focus a replaced or superseded editor', () => {
  const h = harness();
  h.window.__SGT_TOPOLOGY_EPOCH__ = '2';
  const api = h.window.__SGT_REFINE_EDITOR__;
  const second = { ...h.group, children: [] };
  api.reconcile(h.group, '42', {isEditing: true, editSession: '1', inputText: ''});
  const firstInput = h.group.children[0].children[0];
  let firstFocused = 0;
  firstInput.focus = () => { firstFocused++; };
  api.reconcile(second, '43', {isEditing: true, editSession: '2', inputText: ''});
  const secondInput = second.children[0].children[0];
  let secondFocused = 0;
  secondInput.focus = () => { secondFocused++; };
  h.frames[0]();
  assert.equal(h.messages.some(m => m.editSession === '1' && m.action === 'request_refine_focus'), false);
  api.nativeFocusGranted('42', '1', '2');
  api.nativeFocusGranted('42', '1', '1');
  assert.equal(firstFocused, 0);
  api.nativeFocusGranted('43', '2', '2');
  assert.equal(secondFocused, 1);
});

test('removing a control group prunes its editor and releases native focus', () => {
  const h = harness();
  const api = h.window.__SGT_REFINE_EDITOR__;
  api.reconcile(h.group, '42', {isEditing: true, editSession: '1', inputText: ''});
  h.frames[0]();
  h.group.dataset.hwnd = '42';
  let removed = false;
  h.group.remove = () => { removed = true; };
  const container = { querySelectorAll: () => removed ? [] : [h.group] };
  h.window.addEventListener = () => {};
  h.window.__SGT_PROCESSING_CONTROLS__ = { retain() {} };
  const context = vm.createContext({window: h.window,
    document: {fonts: {addEventListener() {}}, getElementById: () => container},
    ResizeObserver: class { constructor() {} unobserve() {} },
    requestAnimationFrame() {}, lastVisibleState: new Map(), lastSentRegions: new Map(),
    updateButtonOpacity() {}
  });
  vm.runInContext(fs.readFileSync(path.join(__dirname, 'layout_runtime.js'), 'utf8'), context);
  h.window.updateWindows({});
  assert.equal(removed, true);
  assert.equal(api.activeCount(), 0);
  assert.equal(h.messages.at(-2).action, 'release_refine_focus');
  assert.equal(h.messages.at(-1).action, 'update_clickable_regions');
});

