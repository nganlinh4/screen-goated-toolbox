const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const source = fs.readFileSync(path.join(__dirname, 'sw.js'), 'utf8')
  .replace(/\nconnect\(\);\s*$/, '').replace('importScripts("frame_registry.js");', '');
const registrySource = fs.readFileSync(path.join(__dirname, 'frame_registry.js'), 'utf8');
const admissionSource = fs.readFileSync(path.join(__dirname, 'cdp_admission.js'), 'utf8');
const revocationSource = fs.readFileSync(path.join(__dirname, 'debugger_revocation.js'), 'utf8');
const listener = () => ({ addListener(fn) { this.handler = fn; } });

function harness({ fail = null, detachHook = null, activeHook = null, commandHook = null, attachHook = null, orphaned = [] } = {}) {
  const events = listener(), detached = listener(), removed = listener();
  const calls = [];
  const owners = new Map();
  const attached = new Set(orphaned);
  let nextNode = 1;
  const key = (tabId, sessionId) => `${tabId}:${sessionId || ''}`;
  function addOwner(tabId, parent, frameId) {
    const group = key(tabId, parent);
    if (!owners.has(group)) owners.set(group, new Map());
    owners.get(group).set(nextNode++, frameId);
  }
  const chrome = {
    storage: {
      local: { async get() { return {}; }, async set() {}, async remove() {} },
      session: { async get() { return {}; }, async set() {} },
    },
    debugger: {
      onEvent: events, onDetach: detached,
      async attach(target) { calls.push(['attach', target.tabId]); if (attachHook) await attachHook(target); if (attached.has(target.tabId)) throw new Error('already attached'); attached.add(target.tabId); },
      async detach(target) { calls.push(['detach', target.tabId]); if (detachHook) await detachHook(target); if (!attached.delete(target.tabId)) throw new Error('not attached'); },
      async sendCommand(target, method, params) {
        calls.push(['cdp', target.tabId, target.sessionId || null, method]);
        if (!attached.has(target.tabId)) throw new Error('not attached');
        if (commandHook) await commandHook(target, method);
        if (method === 'Target.setAutoAttach' && fail && fail(target)) throw new Error('attach failed');
        if (method === 'Page.getFrameTree') return { frameTree: { frame: { id: 'root' } } };
        if (method === 'DOM.getDocument') return { root: { nodeId: 100000 } };
        const group = owners.get(key(target.tabId, target.sessionId)) || new Map();
        if (method === 'DOM.performSearch') return { searchId: 'search', resultCount: group.size };
        if (method === 'DOM.getSearchResults') return { nodeIds: [...group.keys()] };
        if (method === 'DOM.describeNode') return { node: { nodeName: 'IFRAME', frameId: group.get(params.nodeId) } };
        if (method === 'DOM.discardSearchResults') return {};
      },
    },
    tabs: { onRemoved: removed, async query() { if (activeHook) await activeHook(); return [{ id: 3 }]; } },
    runtime: { onStartup: listener(), onInstalled: listener(), onMessage: listener() },
  };
  const api = vm.runInNewContext(revocationSource + '\n' + admissionSource + '\n' + registrySource + '\n' + source + '\n({ handleFrames, ensureAttached, acquireAttachment, detachAll })', {
    chrome, importScripts() {}, self: {}, setInterval() {}, setTimeout, clearTimeout,
    console, Map, Set, URL, crypto: require('node:crypto').webcrypto,
  });
  const attach = (tabId, sessionId, parent = null, existingOwner = false) => {
    if (!existingOwner) addOwner(tabId, parent, sessionId);
    events.handler({ tabId, ...(parent ? { sessionId: parent } : {}) },
      'Target.attachedToTarget', { sessionId, targetInfo: { type: 'iframe', targetId: sessionId } });
  };
  const detach = (tabId, sessionId) => {
    for (const [group, entries] of owners) if (group.startsWith(`${tabId}:`)) {
      for (const [node, id] of entries) if (id === sessionId) entries.delete(node);
    }
    events.handler({ tabId }, 'Target.detachedFromTarget', { sessionId });
  };
  const snapshot = (tabId, requireActive = false) => new Promise(resolve => api.handleFrames({ id: 1, tabId, requireActive }, resolve));
  const unexpectedDetach = (tabId) => { attached.delete(tabId); detached.handler({ tabId }, 'target_closed'); };
  return { api, attach, detach, snapshot, detached, calls, addOwner, unexpectedDetach };
}

test('a canceled attach does not detach a session claimed by a later waiter', async () => {
  let releaseAttach, cancelFirst;
  const stalled = new Promise(resolve => { releaseAttach = resolve; });
  const canceled = new Promise((_, reject) => { cancelFirst = () => reject(new Error('cancelled')); });
  let firstValid = true;
  const h = harness({ attachHook: () => stalled });
  const first = h.api.acquireAttachment(3, promise => Promise.race([promise, canceled]), () => firstValid).catch(() => {});
  const second = h.api.acquireAttachment(3, promise => promise, () => true);
  firstValid = false;
  cancelFirst();
  await first;
  releaseAttach();
  await second;
  await new Promise(resolve => setTimeout(resolve, 10));
  assert.equal(h.calls.filter(call => call[0] === 'attach').length, 1);
  assert.equal(h.calls.some(call => call[0] === 'detach'), false);
  assert.equal((await h.snapshot(3)).ok, true);
});

test('cancel during a permanently stalled auto-attach releases the acquired root', async () => {
  let release, cancel;
  const stalled = new Promise(resolve => { release = resolve; });
  const revoked = new Promise((_, reject) => { cancel = () => reject(new Error('cancelled')); });
  let valid = true;
  const h = harness({ commandHook: async (_, method) => { if (method === 'Target.setAutoAttach') await stalled; } });
  const request = h.api.acquireAttachment(3, p => Promise.race([p, revoked]), () => valid).catch(() => {});
  while (!h.calls.some(call => call[3] === 'Target.setAutoAttach')) await new Promise(r => setTimeout(r, 1));
  valid = false;
  cancel();
  await request;
  await new Promise(resolve => setTimeout(resolve, 10));
  assert.equal(h.calls.some(call => call[0] === 'detach' && call[1] === 3), true);
  release();
});

test('frame inventory timeout releases a root while auto-attach is stalled', async () => {
  let release;
  const stalled = new Promise(resolve => { release = resolve; });
  const h = harness({ commandHook: (_target, method) => method === 'Target.setAutoAttach' ? stalled : undefined });
  const result = await h.snapshot(3);
  assert.equal(result.ok, false);
  await new Promise(resolve => setTimeout(resolve, 10));
  assert.equal(h.calls.some(call => call[0] === 'detach' && call[1] === 3), true);
  release();
});

test('canceled recovery releases its new attachment after an unexpected detach', async () => {
  let attachCount = 0, release, cancel;
  const stalled = new Promise(resolve => { release = resolve; });
  const revoked = new Promise((_, reject) => { cancel = () => reject(new Error('cancelled')); });
  const h = harness({ attachHook: () => ++attachCount === 2 ? stalled : undefined });
  await h.api.ensureAttached(3);
  h.unexpectedDetach(3);
  let valid = true;
  const pending = h.api.acquireAttachment(3, promise => Promise.race([promise, revoked]), () => valid).catch(() => {});
  while (attachCount < 2) await new Promise(resolve => setTimeout(resolve, 1));
  valid = false;
  cancel();
  await pending;
  release();
  await new Promise(resolve => setTimeout(resolve, 10));
  assert.equal(h.calls.some(call => call[0] === 'detach' && call[1] === 3), true);
});

test('canceled waiter behind a pending detach releases only its new attachment', async () => {
  let releaseDetach, cancel, detaches = 0;
  const stalled = new Promise(resolve => { releaseDetach = resolve; });
  const revoked = new Promise((_, reject) => { cancel = () => reject(new Error('cancelled')); });
  let valid = true;
  const h = harness({ detachHook: () => ++detaches === 1 ? stalled : undefined });
  await h.api.ensureAttached(3);
  const oldDetach = h.api.detachAll();
  const waiter = h.api.acquireAttachment(3, promise => Promise.race([promise, revoked]), () => valid).catch(() => {});
  valid = false;
  cancel();
  await waiter;
  releaseDetach();
  await oldDetach;
  await new Promise(resolve => setTimeout(resolve, 15));
  assert.equal(h.calls.filter(call => call[0] === 'attach').length, 2);
  assert.equal(detaches, 2);
});

test('nested sessions survive host reconnect and remain scoped to their root tab', async () => {
  const h = harness();
  await h.api.ensureAttached(3);
  await h.api.ensureAttached(4);
  h.attach(3, 'child');
  h.attach(3, 'nested', 'child');
  h.attach(4, 'other');
  const first = await h.snapshot(3);
  assert.equal(first.ok, true);
  assert.deepEqual(Array.from(first.result.sessions), ['child', 'nested']);
  assert.ok(h.calls.some(call => call[0] === 'cdp' && call[1] === 3 && call[2] === 'child' && call[3] === 'Target.setAutoAttach'));
  const afterReconnect = await h.snapshot(3);
  assert.equal(afterReconnect.result.registryId, first.result.registryId);
  assert.deepEqual(Array.from(afterReconnect.result.sessions), ['child', 'nested']);
  h.detach(3, 'child');
  const afterDetach = await h.snapshot(3);
  assert.deepEqual(Array.from(afterDetach.result.sessions), []);
  assert.ok(afterDetach.result.generation > first.result.generation);
  assert.deepEqual(Array.from((await h.snapshot(4)).result.sessions), ['other']);
});

test('failed child auto-attachment reports incomplete coverage', async () => {
  const h = harness({ fail: target => target.sessionId === 'child' });
  await h.api.ensureAttached(3);
  h.attach(3, 'child');
  const result = await h.snapshot(3);
  assert.equal(result.ok, false);
  assert.equal(result.code, 'ERR_BROWSER_FRAME_COVERAGE_INCOMPLETE');
});

test('overflow is explicitly refused rather than silently truncated', async () => {
  const h = harness();
  await h.api.ensureAttached(3);
  for (let i = 0; i < 129; i++) h.attach(3, `child-${i}`);
  const result = await h.snapshot(3);
  assert.equal(result.ok, false);
  assert.equal(result.code, 'ERR_BROWSER_TOO_MANY_FRAMES');
});

test('child sessions beyond the bound refuse the page with a distinct code', async () => {
  let h, emitted = false;
  h = harness({ commandHook: async (target, method) => {
    if (method !== 'Target.setAutoAttach' || target.sessionId) return;
    // Chrome re-announces every existing child after each root auto-attach.
    for (let i = 0; i < 129; i++) h.attach(3, `child-${i}`, null, emitted);
    emitted = true;
  } });
  const result = await h.snapshot(3);
  assert.equal(result.ok, false);
  assert.equal(result.code, 'ERR_BROWSER_TOO_MANY_FRAMES');
  assert.match(result.error, /more than 128 frames/);
});

test('a transient root auto-attach failure rebuilds a complete registry on retry', async () => {
  let fail = true;
  const h = harness({ fail: target => !target.sessionId && fail });
  assert.equal((await h.snapshot(3)).ok, false);
  fail = false;
  assert.equal((await h.snapshot(3)).ok, true);
  assert.equal(h.calls.filter(call => call[0] === 'attach').length, 2);
});

test('overflow recovers only after a fresh registry and current frames reconcile', async () => {
  const h = harness();
  await h.api.ensureAttached(3);
  for (let i = 0; i < 129; i++) h.attach(3, `child-${i}`);
  assert.equal((await h.snapshot(3)).ok, false);
  for (let i = 0; i < 129; i++) h.detach(3, `child-${i}`);
  assert.equal((await h.snapshot(3)).ok, true);
  assert.ok(h.calls.filter(call => call[0] === 'attach').length >= 2);
});

test('debugger reattachment creates a fresh registry identity', async () => {
  const h = harness();
  const first = await h.snapshot(3);
  await h.api.detachAll();
  const second = await h.snapshot(3);
  assert.equal(second.ok, true);
  assert.notEqual(second.result.registryId, first.result.registryId);
});

test('unattached or same-process frames fail closed until attachment arrives', async () => {
  const h = harness();
  await h.api.ensureAttached(3);
  h.addOwner(3, null, 'frame');
  const before = await h.snapshot(3);
  assert.equal(before.ok, false);
  assert.equal(before.code, 'ERR_BROWSER_FRAME_COVERAGE_INCOMPLETE');
  h.attach(3, 'frame', null, true);
  assert.equal((await h.snapshot(3)).ok, true);
});

test('same-process frames inside an attached child are not silently omitted', async () => {
  const h = harness();
  await h.api.ensureAttached(3);
  h.attach(3, 'child');
  h.addOwner(3, 'child', 'same-process-frame');
  const result = await h.snapshot(3);
  assert.equal(result.ok, false);
  assert.equal(result.code, 'ERR_BROWSER_FRAME_COVERAGE_INCOMPLETE');
});

test('a delayed detach cannot clear a newly reattached registry', async () => {
  let release;
  const wait = new Promise(resolve => { release = resolve; });
  const h = harness({ detachHook: () => wait });
  const first = await h.snapshot(3);
  const oldDetach = h.api.detachAll();
  const reattach = h.api.ensureAttached(3);
  await Promise.resolve();
  assert.equal(h.calls.filter(call => call[0] === 'attach').length, 1);
  h.detached.handler({ tabId: 3 }, 'target_closed');
  release();
  await Promise.all([oldDetach, reattach]);
  const second = await h.snapshot(3);
  assert.equal(second.ok, true);
  assert.notEqual(second.result.registryId, first.result.registryId);
  assert.equal(h.calls.filter(call => call[0] === 'attach').length, 2);
});

test('a frame arriving during the final active-tab check invalidates coverage', async () => {
  let calls = 0;
  let h;
  h = harness({ activeHook: async () => { if (++calls === 2) h.attach(3, 'late'); } });
  const result = await h.snapshot(3, true);
  assert.equal(result.ok, false);
  assert.equal(result.code, 'ERR_BROWSER_FRAME_COVERAGE_INCOMPLETE');
});

test('a rejected detach reconciles still-owned debugger without reattaching', async () => {
  const h = harness({ detachHook: async () => { throw new Error('still attached'); } });
  assert.equal((await h.snapshot(3)).ok, true);
  await h.api.detachAll();
  const after = await h.snapshot(3);
  assert.equal(after.ok, true);
  assert.equal(h.calls.filter(call => call[0] === 'attach').length, 1);
});

test('late detach notification reconciles the live replacement without reattaching', async () => {
  const h = harness();
  assert.equal((await h.snapshot(3)).ok, true);
  await h.api.detachAll();
  assert.equal((await h.snapshot(3)).ok, true);
  h.detached.handler({ tabId: 3 }, 'late');
  assert.equal((await h.snapshot(3)).ok, true);
  assert.equal(h.calls.filter(call => call[0] === 'attach').length, 2);
});

test('a stale detach event during a newer failed detach preserves live ownership', async () => {
  let count = 0, rejectSecond, startedSecond;
  const secondStarted = new Promise(resolve => { startedSecond = resolve; });
  const second = new Promise((_, reject) => { rejectSecond = reject; });
  const h = harness({ detachHook: () => {
    if (++count === 2) { startedSecond(); return second; }
  } });
  await h.api.ensureAttached(3);
  await h.api.detachAll();
  await h.api.ensureAttached(3);
  const pending = h.api.detachAll();
  await secondStarted;
  h.detached.handler({ tabId: 3 }, 'old-session');
  rejectSecond(new Error('still attached'));
  await pending;
  await h.api.ensureAttached(3);
  assert.equal(h.calls.filter(call => call[0] === 'attach').length, 2);
  assert.equal((await h.snapshot(3)).ok, true);
});

test('unexpected debugger detach reattaches and rebuilds its registry', async () => {
  const h = harness();
  const first = await h.snapshot(3);
  h.unexpectedDetach(3);
  const recovered = await h.snapshot(3);
  assert.equal(recovered.ok, true);
  assert.notEqual(recovered.result.registryId, first.result.registryId);
  assert.equal(h.calls.filter(call => call[0] === 'attach').length, 2);
});

test('a transient ownership probe failure remains recoverable without detaching', async () => {
  let failProbe = false;
  const h = harness({ commandHook: async (_, method) => {
    if (failProbe && method === 'Page.getFrameTree') { failProbe = false; throw new Error('temporary probe failure'); }
  } });
  const before = await h.snapshot(3);
  h.detached.handler({ tabId: 3 }, 'late');
  failProbe = true;
  assert.equal((await h.snapshot(3)).code, 'ERR_BROWSER_FRAME_COVERAGE_INCOMPLETE');
  const recovered = await h.snapshot(3);
  assert.equal(recovered.ok, true);
  assert.equal(recovered.result.registryId, before.result.registryId);
});

test('timed-out inventory cannot start more work or accumulate new jobs', async () => {
  let release;
  const wait = new Promise(resolve => { release = resolve; });
  const h = harness({ commandHook: async (_, method) => { if (method === 'DOM.getDocument') await wait; } });
  const first = await h.snapshot(3);
  assert.equal(first.code, 'ERR_BROWSER_FRAME_COVERAGE_INCOMPLETE');
  const repeated = await h.snapshot(3);
  assert.equal(repeated.code, 'ERR_BROWSER_FRAME_COVERAGE_INCOMPLETE');
  assert.equal(h.calls.filter(call => call[3] === 'DOM.getDocument').length, 1);
  release();
  await new Promise(resolve => setTimeout(resolve, 20));
  assert.equal(h.calls.some(call => call[3] === 'DOM.performSearch'), false);
});

test('a debugger session left by an earlier worker is reclaimed instead of blocking its tab', async () => {
  const h = harness({ orphaned: [3] });
  const result = await h.snapshot(3);
  assert.equal(result.ok, true);
  assert.deepEqual(h.calls.filter(call => call[0] === 'attach' || call[0] === 'detach').map(call => call[0]),
    ['attach', 'detach', 'attach']);
});

test('an attach failure on a tab this extension does not debug is reported unchanged', async () => {
  const h = harness({ attachHook: async () => { throw new Error('cannot attach to this target'); } });
  const result = await h.snapshot(3);
  assert.equal(result.ok, false);
  assert.match(result.error, /cannot attach to this target/);
  assert.equal(h.calls.filter(call => call[0] === 'attach').length, 1);
});
