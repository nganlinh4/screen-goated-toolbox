const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const test = require('node:test');
const assert = require('node:assert/strict');

function harness() {
  const messages = [], frames = [], calls = [];
  const window = {
    ipc: { postMessage: raw => messages.push(JSON.parse(raw)) },
    __SGT_VERIFY_SCENE_GEOMETRY__: () => calls.push('verify'),
    invalidateButtonRegions: () => calls.push('invalidate-buttons'),
    __SGT_PROCESSING_CONTROLS__: { invalidate: () => calls.push('invalidate-processing') },
    __SGT_BUTTON_SCENE__: { resetTopology: () => calls.push('reset-gesture'), rebuild: () => calls.push('rebuild') },
    updateButtonOpacity: force => { calls.push('regions'); messages.push({action:'update_clickable_regions', topology_epoch:window.__SGT_TOPOLOGY_EPOCH__, force}); },
    __SGT_REFINE_EDITOR__: {
      republishDrafts: () => { calls.push('draft'); messages.push({action:'update_refine_draft', topology_epoch:window.__SGT_TOPOLOGY_EPOCH__}); },
      republishFocus: () => { calls.push('focus'); messages.push({action:'request_refine_focus', topology_epoch:window.__SGT_TOPOLOGY_EPOCH__}); }
    },
  };
  const context = vm.createContext({window, requestAnimationFrame: fn => frames.push(fn)});
  vm.runInContext(fs.readFileSync(path.join(__dirname, 'scene_batch_runtime.js'), 'utf8'), context);
  return {window, frames, calls, messages, frame() { assert.ok(frames.length); frames.shift()(); }};
}

test('scene batches acknowledge only successful application and verification', () => {
  const h = harness();
  h.window.__SGT_APPLY_SCENE_BATCH__(() => h.calls.push('apply'), null, null, '42');
  assert.deepEqual(h.calls, ['apply', 'verify']);
  assert.deepEqual(h.messages, [{action:'scene_batch_complete', batch_id:'42'}]);
  h.window.__SGT_APPLY_SCENE_BATCH__(() => { throw Error('failed'); }, null, null, '43');
  assert.equal(h.messages.at(-1).type, 'command_error');
  assert.equal(h.messages.some(message => message.batch_id === '43'), false);
});

test('rebase publishes fresh regions and drafts before readiness after two frames', () => {
  const h = harness();
  assert.equal(h.window.__SGT_TOPOLOGY_EPOCH__, '1');
  h.window.__SGT_REBASE_TOPOLOGY__('2', () => h.calls.push('apply'));
  assert.equal(h.messages.length, 0);
  h.frame(); assert.equal(h.messages.length, 0);
  h.frame();
  assert.deepEqual(h.messages.map(message => message.action), ['update_clickable_regions', 'update_refine_draft', 'topology_ready', 'request_refine_focus']);
  assert.equal(h.messages[0].topology_epoch, '2');
  assert.equal(h.messages[1].topology_epoch, '2');
  assert.equal(h.messages[2].epoch, '2');
  assert.ok(h.calls.indexOf('reset-gesture') < h.calls.indexOf('apply'));
});

test('superseded topology cannot publish stale regions or readiness', () => {
  const h = harness();
  h.window.__SGT_REBASE_TOPOLOGY__('2', () => {});
  h.window.__SGT_REBASE_TOPOLOGY__('3', () => {});
  h.frame(); h.frame(); h.frame(); h.frame();
  assert.deepEqual(h.messages.map(message => message.action), ['update_clickable_regions', 'update_refine_draft', 'topology_ready', 'request_refine_focus']);
  assert.equal(h.messages[2].epoch, '3');
});

test('isolated actions use the current epoch and owning card while stale actions have no effect', () => {
  const messages = [], raised = [];
  const window = {__SGT_TOPOLOGY_EPOCH__: '2', addEventListener() {},
    ipc: {postMessage: raw => messages.push(JSON.parse(raw))}};
  const context = vm.createContext({window, raiseCard: entry => raised.push(entry)});
  const source = fs.readFileSync(path.join(__dirname, 'scene_frame_runtime.js'), 'utf8');
  vm.runInContext(source, context);
  const entry = {card: {dataset: {id: '7'}}};
  for (const type of ['card_interaction', 'card_navigation', 'copy_selection']) {
    context.handleIsolatedCardMessage('7', entry, {type, topology_epoch:'1', url:'https://example.test', text:'text'});
  }
  assert.deepEqual(raised, []);
  assert.deepEqual(messages, []);
  context.handleIsolatedCardMessage('7', entry, {type:'card_interaction', topology_epoch:'2'});
  assert.equal(raised.length, 1);
  assert.equal(messages[0].topology_epoch, '2');
  context.handleIsolatedCardMessage('7', entry, {type:'card_navigation', topology_epoch:'2', url:'https://example.test/next'});
  context.handleIsolatedCardMessage('7', entry, {type:'copy_selection', topology_epoch:'2', text:'selected text'});
  assert.deepEqual(messages, [
    {type:'interaction', id:7, topology_epoch:'2'},
    {type:'navigation_request', id:7, url:'https://example.test/next', topology_epoch:'2'},
    {action:'copy_selection', hwnd:'7', text:'selected text', topology_epoch:'2'},
  ]);
});
