const test = require('node:test');
const assert = require('node:assert/strict');
const { harness, challenge, frame, decrypt, until } = require('./bridge_harness.cjs');

async function activate(body, options = {}) {
  const h = await harness({ secret: 'durable', ...options });
  const state = challenge(h.sock, 'durable', 'secret');
  await until(() => h.sock.sent.length === 2);
  h.sock.receive(frame('durable', state, 1, { type: 'ready' }));
  await until(() => h.sock.sent.length === 3 && h.sock.snapshots.length === 1);
  h.sock.receive(frame('durable', state, 2, { type: 'tabs', action: 'activate', tabId: 12, id: 61, effectToken: 'effect-focus', timeoutMs: 10000, ...body }));
  await until(() => h.sock.sent.length === 4);
  assert.equal(h.operations.some(op => op[0] === 'windows.update'), false, 'focus waits for the grant');
  h.sock.receive(frame('durable', state, 3, { type: 'cdp_dispatch_grant', requestId: 61, remainingMs: 5000 }));
  const replies = () => h.sock.sent.slice(4).map(value => decrypt('durable', state, value));
  await until(() => replies().some(value => value.id === 61) && replies().some(value => value.type === 'effect_terminal'));
  return { h, reply: replies().find(value => value.id === 61) };
}

test('a switch that asks for focus brings the tab window to the front and reports it', async () => {
  const { h, reply } = await activate({ focusWindow: true });
  assert.deepEqual(reply.result, { windowFocused: true });
  // Operations come from the worker's VM realm; compare their JSON shape.
  const updates = JSON.parse(JSON.stringify(h.operations.filter(op => op[0] === 'tabs.update' || op[0] === 'windows.update')));
  assert.deepEqual(updates, [['tabs.update', 12, { active: true }], ['windows.update', 5, { focused: true }]]);
});

test('a refused window focus is reported, not claimed', async () => {
  const { reply } = await activate({ focusWindow: true }, { windowFocusGranted: false });
  assert.equal(reply.ok, true);
  assert.deepEqual(reply.result, { windowFocused: false });
});

test('a plain activation never moves window focus', async () => {
  const { h, reply } = await activate({});
  assert.deepEqual(reply.result, {});
  assert.equal(h.operations.some(op => op[0] === 'windows.update'), false);
});

test('the worker advertises window focus on activation', async () => {
  const h = await harness({ secret: 'durable' });
  assert.equal(h.sock.sent[0].type, 'hello');
  assert.ok(h.sock.sent[0].capabilities.includes('tabs.activate.focus_window'));
});
