const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const source = fs.readFileSync(path.join(__dirname, 'sw.js'), 'utf8');
const handlerSource = source.slice(source.indexOf('async function handleCdp('), source.indexOf('// Tab list / activate'));

function harness(activeTabs = [12, 12]) {
  const calls = [];
  const replies = [];
  const context = {
    debuggerRevocations: { assertAllowed() {} },
    debuggerCommand: (target, method, params) => context.chrome.debugger.sendCommand(target, method, params),
    activeTabId: async () => activeTabs.shift() ?? 12,
    acquireAttachment: async (tabId) => { calls.push({ attach: tabId }); return { retain() {}, release() {} }; },
    chrome: { debugger: { sendCommand: async (target, method, params) => {
      calls.push({ target: JSON.parse(JSON.stringify(target)), method, params });
      return { delivered: true };
    } } },
  };
  const handleCdp = vm.runInNewContext(`${handlerSource}\nhandleCdp`, context);
  const admission = { requestGrant: async () => true, valid: () => true, wait: p => p, invoke: f => f() };
  return { calls, replies, send: (message) => handleCdp(message, (reply) => replies.push(reply), admission) };
}

test('child command retains its root tab alongside the session', async () => {
  const h = harness();
  await h.send({ id: 1, tabId: 12, sessionId: 'child-session', method: 'Runtime.evaluate', params: { expression: '1' } });
  assert.deepEqual(h.calls[1].target, { tabId: 12, sessionId: 'child-session' });
  assert.equal(h.replies[0].ok, true);
});

test('root commands use only the selected tab', async () => {
  const h = harness();
  await h.send({ id: 2, tabId: 12, method: 'Runtime.evaluate' });
  assert.deepEqual(h.calls[1].target, { tabId: 12 });
});

test('active-tab drift prevents dispatch to a child session', async () => {
  const h = harness([12, 13]);
  await h.send({ id: 3, tabId: 12, sessionId: 'child-session', requireActive: true, method: 'Runtime.evaluate' });
  assert.equal(h.calls.length, 1);
  assert.equal(h.replies[0].ok, false);
});
