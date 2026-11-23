const test = require('node:test');
const assert = require('node:assert/strict');
const vm = require('node:vm');
const { harness, challenge, frame, decrypt, hmac, sleep, until, admissionSource } = require('./bridge_harness.cjs');

test('unauthenticated commands, pair, and debugger events never cross the bridge', async () => {
  for (const forged of [
    { type: 'cdp', id: 1, method: 'Runtime.evaluate' },
    { type: 'tabs', id: 2, action: 'list' },
    { type: 'runtime', id: 3, action: 'reload' },
    { type: 'pair', secret: 'attacker' },
  ]) {
    const h = await harness({ secret: 'durable' });
    h.debuggerEvents.handler({ tabId: 1 }, 'Network.responseReceived', { secret: 'private' });
    assert.equal(h.sock.sent.length, 1);
    h.sock.receive(forged);
    await until(() => h.sock.readyState === 3);
    assert.deepEqual(h.operations, []);
    assert.equal(h.stored.secretV8, 'durable');
    assert.equal(h.sock.sent.length, 1);
  }
});

test('cancellation reaches a CDP request while debugger attach is stalled', async () => {
  let release;
  const stalled = new Promise(resolve => { release = resolve; });
  const h = await harness({ secret: 'durable', attachHook: () => stalled });
  const state = challenge(h.sock, 'durable', 'secret');
  await until(() => h.sock.sent.length === 2);
  h.sock.receive(frame('durable', state, 1, { type: 'ready' }));
  await until(() => h.sock.sent.length === 3);
  h.sock.receive(frame('durable', state, 2, { type: 'cdp', id: 42, effectToken: 'test-42', tabId: 12, timeoutMs: 10000, method: 'Input.dispatchMouseEvent' }));
  await until(() => h.operations.some(op => op[0] === 'attach'));
  h.sock.receive(frame('durable', state, 3, { type: 'request_cancel', requestId: 42 }));
  await sleep(); await sleep();
  release();
  await until(() => h.sock.sent.length >= 4);
  const response = decrypt('durable', state, h.sock.sent[3]);
  assert.equal(response.id, 42);
  assert.equal(response.ok, false);
  assert.equal(h.operations.some(op => op[0] === 'cdp' && op[2] === 'Input.dispatchMouseEvent'), false);
  await until(() => h.timers.some(timer => timer.ms === 0 && !timer.cleared));
  for (const timer of h.timers.filter(timer => timer.ms === 0 && !timer.cleared)) { timer.cleared = true; timer.fn(); }
  await until(() => h.operations.some(op => op[0] === 'detach' && op[1].tabId === 12));
});

test('CDP sends its command only after the authenticated host grant', async () => {
  const h = await harness({ secret: 'durable' });
  const state = challenge(h.sock, 'durable', 'secret');
  await until(() => h.sock.sent.length === 2);
  h.sock.receive(frame('durable', state, 1, { type: 'ready' }));
  await until(() => h.sock.sent.length === 3);
  h.sock.receive(frame('durable', state, 2, { type: 'cdp', id: 43, effectToken: 'test-43', tabId: 12, timeoutMs: 10000, method: 'Input.dispatchMouseEvent' }));
  await until(() => h.sock.sent.length === 4);
  assert.deepEqual(decrypt('durable', state, h.sock.sent[3]), { type: 'cdp_dispatch_ready', requestId: 43 });
  assert.equal(h.operations.some(op => op[0] === 'cdp' && op[2] === 'Input.dispatchMouseEvent'), false);
  h.sock.receive(frame('durable', state, 3, { type: 'cdp_dispatch_grant', requestId: 43, remainingMs: 5000 }));
  await until(() => h.sock.sent.length >= 5);
  assert.equal(decrypt('durable', state, h.sock.sent[4]).ok, true);
  assert.equal(h.operations.filter(op => op[0] === 'cdp' && op[2] === 'Input.dispatchMouseEvent').length, 1);
});

test('a denied admission never dispatches the CDP command', async () => {
  const h = await harness({ secret: 'durable' });
  const state = challenge(h.sock, 'durable', 'secret');
  await until(() => h.sock.sent.length === 2);
  h.sock.receive(frame('durable', state, 1, { type: 'ready' }));
  await until(() => h.sock.sent.length === 3);
  h.sock.receive(frame('durable', state, 2, { type: 'cdp', id: 45, effectToken: 'test-45', tabId: 12, timeoutMs: 10000, method: 'Input.dispatchMouseEvent' }));
  await until(() => h.sock.sent.length === 4);
  h.sock.receive(frame('durable', state, 3, { type: 'cdp_dispatch_grant', requestId: 45, remainingMs: 0 }));
  await until(() => h.sock.sent.length >= 5);
  assert.equal(decrypt('durable', state, h.sock.sent[4]).ok, false);
  assert.equal(h.operations.some(op => op[0] === 'cdp' && op[2] === 'Input.dispatchMouseEvent'), false);
  await until(() => h.operations.some(op => op[0] === 'detach' && op[1].tabId === 12));
});

test('cancellation while waiting for grant releases a newly acquired debugger', async () => {
  const h = await harness({ secret: 'durable' });
  const state = challenge(h.sock, 'durable', 'secret');
  await until(() => h.sock.sent.length === 2);
  h.sock.receive(frame('durable', state, 1, { type: 'ready' }));
  await until(() => h.sock.sent.length === 3);
  h.sock.receive(frame('durable', state, 2, { type: 'cdp', id: 47, effectToken: 'cancel-grant', tabId: 12, timeoutMs: 10000, method: 'Input.dispatchMouseEvent' }));
  await until(() => h.sock.sent.some((item, index) => index >= 3 && decrypt('durable', state, item).type === 'cdp_dispatch_ready'));
  h.sock.receive(frame('durable', state, 3, { type: 'request_cancel', requestId: 47 }));
  await until(() => h.timers.some(timer => timer.ms === 0 && !timer.cleared));
  for (const timer of h.timers.filter(timer => timer.ms === 0 && !timer.cleared)) { timer.cleared = true; timer.fn(); }
  await until(() => h.operations.some(op => op[0] === 'detach' && op[1].tabId === 12));
  assert.equal(h.operations.some(op => op[0] === 'cdp' && op[2] === 'Input.dispatchMouseEvent'), false);
});

test('disconnect revokes a CDP request stalled during attachment', async () => {
  let release;
  const stalled = new Promise(resolve => { release = resolve; });
  const h = await harness({ secret: 'durable', attachHook: () => stalled });
  const state = challenge(h.sock, 'durable', 'secret');
  await until(() => h.sock.sent.length === 2);
  h.sock.receive(frame('durable', state, 1, { type: 'ready' }));
  await until(() => h.sock.sent.length === 3);
  h.sock.receive(frame('durable', state, 2, { type: 'cdp', id: 46, effectToken: 'test-46', tabId: 12, timeoutMs: 10000, method: 'Input.dispatchMouseEvent' }));
  await until(() => h.operations.some(op => op[0] === 'attach'));
  h.sock.close();
  release();
  await sleep(); await sleep();
  assert.equal(h.operations.some(op => op[0] === 'cdp' && op[2] === 'Input.dispatchMouseEvent'), false);
});

test('cancellation after grant but during final tab check prevents dispatch', async () => {
  let checks = 0, release;
  const stalled = new Promise(resolve => { release = resolve; });
  const h = await harness({ secret: 'durable', queryHook: async () => { if (++checks === 3) await stalled; return [{ id: 12 }]; } });
  const state = challenge(h.sock, 'durable', 'secret');
  await until(() => h.sock.sent.length === 2);
  h.sock.receive(frame('durable', state, 1, { type: 'ready' }));
  await until(() => h.sock.sent.length === 3);
  h.sock.receive(frame('durable', state, 2, { type: 'cdp', id: 44, effectToken: 'test-44', tabId: 12, requireActive: true, timeoutMs: 10000, method: 'Input.dispatchMouseEvent' }));
  await until(() => h.sock.sent.length === 4);
  h.sock.receive(frame('durable', state, 3, { type: 'cdp_dispatch_grant', requestId: 44, remainingMs: 5000 }));
  await until(() => checks === 3);
  h.sock.receive(frame('durable', state, 4, { type: 'request_cancel', requestId: 44 }));
  await sleep(); await sleep();
  release();
  await until(() => h.sock.sent.length >= 5);
  assert.equal(decrypt('durable', state, h.sock.sent[4]).ok, false);
  assert.equal(h.operations.some(op => op[0] === 'cdp' && op[2] === 'Input.dispatchMouseEvent'), false);
});

test('canceled queued tab effect and stalled preparation terminate without late dispatch', async () => {
  let release;
  const stalled = new Promise(resolve => { release = resolve; });
  const h = await harness({ secret: 'durable', attachHook: () => stalled });
  const state = challenge(h.sock, 'durable', 'secret');
  await until(() => h.sock.sent.length === 2);
  h.sock.receive(frame('durable', state, 1, { type: 'ready' }));
  await until(() => h.sock.sent.length === 3);
  h.sock.receive(frame('durable', state, 2, { type: 'cdp', id: 50, effectToken: 'queued-50', tabId: 12, timeoutMs: 10000, method: 'Runtime.evaluate' }));
  await until(() => h.operations.some(op => op[0] === 'attach'));
  h.sock.receive(frame('durable', state, 3, { type: 'tabs', id: 51, effectToken: 'queued-51', timeoutMs: 10000, action: 'create', url: 'https://example.com/' }));
  h.sock.receive(frame('durable', state, 4, { type: 'request_cancel', requestId: 51 }));
  h.sock.receive(frame('durable', state, 5, { type: 'request_cancel', requestId: 50 }));
  await until(() => h.sock.sent.length >= 6);
  const terminals = h.sock.sent.slice(3).map(value => decrypt('durable', state, value))
    .filter(value => value.type === 'effect_terminal');
  assert.deepEqual(terminals.map(value => [value.effectToken, value.effectMayHaveOccurred]).sort(),
    [['queued-50', false], ['queued-51', false]]);
  assert.equal(h.operations.some(op => op[0] === 'tabs.create'), false);
  release();
  await sleep(); await sleep();
  assert.equal(h.operations.some(op => op[0] === 'cdp' && op[2] === 'Runtime.evaluate'), false);
});

test('in-flight effect terminal survives disconnect and is replayed until acknowledged', async () => {
  let release;
  const stalled = new Promise(resolve => { release = resolve; });
  const h = await harness({ secret: 'durable', commandHook: async (_, method) => {
    if (method === 'Runtime.evaluate') await stalled;
    return {};
  } });
  const first = challenge(h.sock, 'durable', 'secret');
  await until(() => h.sock.sent.length === 2);
  h.sock.receive(frame('durable', first, 1, { type: 'ready' }));
  await until(() => h.sock.sent.length === 3);
  h.sock.receive(frame('durable', first, 2, { type: 'cdp', id: 52, effectToken: 'replay-52', tabId: 12, timeoutMs: 10000, method: 'Runtime.evaluate' }));
  await until(() => h.sock.sent.length === 4);
  h.sock.receive(frame('durable', first, 3, { type: 'cdp_dispatch_grant', requestId: 52, remainingMs: 5000 }));
  await until(() => h.operations.some(op => op[0] === 'cdp' && op[2] === 'Runtime.evaluate'));
  h.sock.close();
  h.startup.handler();
  await until(() => h.sockets.length === 2);
  const second = h.sockets[1]; second.open();
  const state = challenge(second, 'durable', 'secret');
  await until(() => second.sent.length === 2);
  second.receive(frame('durable', state, 1, { type: 'ready' }));
  await until(() => second.sent.length === 3);
  await until(() => second.snapshots.length === 1);
  assert.deepEqual(second.snapshots[0], { type: 'effect_ledger_snapshot', active: ['replay-52'] });
  await sleep();
  assert.equal(second.sent.length, 3, 'in-flight effect cannot be reported terminal early');
  release();
  await until(() => second.sent.length >= 4);
  assert.deepEqual(decrypt('durable', state, second.sent[3]),
    { type: 'effect_terminal', effectToken: 'replay-52', effectMayHaveOccurred: true });
  second.receive(frame('durable', state, 2, { type: 'effect_terminal_ack', effectToken: 'replay-52' }));
  await sleep(); await sleep();
  second.close();
  h.startup.handler();
  await until(() => h.sockets.length === 3);
  const third = h.sockets[2]; third.open();
  const next = challenge(third, 'durable', 'secret');
  await until(() => third.sent.length === 2);
  third.receive(frame('durable', next, 1, { type: 'ready' }));
  await until(() => third.sent.length === 3);
  await until(() => third.snapshots.length === 1);
  assert.deepEqual(third.snapshots[0], { type: 'effect_ledger_snapshot', active: [] });
  await sleep(); await sleep();
  assert.equal(third.sent.length, 3, 'acknowledged terminal is not replayed');
});

test('a permanently stalled preparation expires without wedging admission', async () => {
  const emitted = [];
  const api = vm.runInNewContext(admissionSource + '\n({ createEffectLedger, createCdpAdmission })',
    { setTimeout, clearTimeout, performance: require('node:perf_hooks').performance });
  const ledger = api.createEffectLedger(value => emitted.push(value));
  const admission = api.createCdpAdmission(() => true, async () => {}, ledger);
  const first = admission.accept({ id: 71, effectToken: 'expiry-71', timeoutMs: 20 });
  await assert.rejects(first.wait(new Promise(() => {})), /expired/);
  assert.throws(() => first.invoke(() => assert.fail('late effect')), /expired/);
  admission.finish(first);
  assert.deepEqual(JSON.parse(JSON.stringify(emitted)), [
    { type: 'effect_terminal', effectToken: 'expiry-71', effectMayHaveOccurred: false },
  ]);
  assert.ok(admission.accept({ id: 72, effectToken: 'expiry-72', timeoutMs: 20 }));
  admission.close();
});

test('reload termination replays durable terminal on a freshly authenticated worker', async () => {
  const shared = { secretV8: 'durable' };
  let old;
  old = await harness({ sharedStored: shared, reloadHook: () => old.sock.close() });
  const state = challenge(old.sock, 'durable', 'secret');
  await until(() => old.sock.sent.length === 2);
  old.sock.receive(frame('durable', state, 1, { type: 'ready' }));
  await until(() => old.sock.sent.length === 3 && old.sock.snapshots.length === 1);
  old.sock.receive(frame('durable', state, 2, {
    type: 'runtime', action: 'reload', id: 74, effectToken: 'worker-reload-74', timeoutMs: 10000,
  }));
  await until(() => old.sock.sent.length === 4);
  assert.equal(shared.effectReloadHandoffV11.token, 'worker-reload-74');
  old.sock.receive(frame('durable', state, 3, { type: 'effect_terminal_ack', effectToken: 'worker-reload-74' }));
  await sleep(); await sleep();
  assert.equal(shared.effectReloadHandoffV11.token, 'worker-reload-74', 'an early ack cannot erase an active handoff');
  old.sock.receive(frame('durable', state, 4, { type: 'cdp_dispatch_grant', requestId: 74, remainingMs: 5000 }));
  await until(() => old.sock.readyState === 3);
  assert.equal(old.operations.filter(op => op[0] === 'reload').length, 1);

  const next = await harness({ sharedStored: shared });
  const nextState = challenge(next.sock, 'durable', 'secret');
  await until(() => next.sock.sent.length === 2);
  next.sock.receive(frame('durable', nextState, 1, { type: 'ready' }));
  await until(() => next.sock.sent.length >= 4 && next.sock.snapshots.length === 1);
  assert.deepEqual(decrypt('durable', nextState, next.sock.sent[3]), {
    type: 'effect_terminal', effectToken: 'worker-reload-74', effectMayHaveOccurred: true,
  });
  next.sock.receive(frame('durable', nextState, 2, { type: 'effect_terminal_ack', effectToken: 'worker-reload-74' }));
  await until(() => shared.effectReloadHandoffV11 === undefined);
});

for (const [action, body, operation] of [
  ['activate', { type: 'tabs', action: 'activate', tabId: 12 }, 'tabs.update'],
  ['navigate', { type: 'tabs', action: 'navigate', tabId: 12, url: 'https://example.com/next' }, 'tabs.update'],
  ['create', { type: 'tabs', action: 'create', url: 'https://example.com/' }, 'tabs.create'],
  ['remove', { type: 'tabs', action: 'remove', tabId: 12 }, 'tabs.remove'],
]) {
  test(`${action} runs only after an authenticated grant and reports terminal`, async () => {
    const h = await harness({ secret: 'durable' });
    const state = challenge(h.sock, 'durable', 'secret');
    await until(() => h.sock.sent.length === 2);
    h.sock.receive(frame('durable', state, 1, { type: 'ready' }));
    await until(() => h.sock.sent.length === 3 && h.sock.snapshots.length === 1);
    h.sock.receive(frame('durable', state, 2, { ...body, id: 61, effectToken: `effect-${action}`, timeoutMs: 10000 }));
    await until(() => h.sock.sent.length === 4);
    assert.equal(h.operations.some(op => op[0] === operation), false);
    h.sock.receive(frame('durable', state, 3, { type: 'cdp_dispatch_grant', requestId: 61, remainingMs: 5000 }));
    await until(() => h.sock.sent.slice(4).map(value => decrypt('durable', state, value))
      .some(value => value.type === 'effect_terminal'));
    assert.equal(h.operations.filter(op => op[0] === operation).length, 1);
    assert.deepEqual(h.sock.sent.slice(4).map(value => decrypt('durable', state, value))
      .find(value => value.type === 'effect_terminal'),
      { type: 'effect_terminal', effectToken: `effect-${action}`, effectMayHaveOccurred: true });
  });
}

test('forged or reflected host proofs cannot authenticate or elicit a client proof', async () => {
  for (const proof of ['00'.repeat(32), 'client']) {
    const h = await harness({ secret: 'durable' });
    const nonce = 'ab'.repeat(32), client = h.sock.sent[0].clientNonce;
    const mac = proof === 'client' ? hmac('durable', `sgt-browser-v8|client|secret|${client}|${nonce}`) : proof;
    challenge(h.sock, 'durable', 'secret', nonce, mac);
    await until(() => h.sock.readyState === 3);
    assert.equal(h.sock.sent.length, 1);
    assert.deepEqual(h.operations, []);
  }
});

test('bootstrap pairing derives a durable secret and protects ordered frames and events', async () => {
  const h = await harness({ bootstrap: 'bootstrap-key' });
  const key = hmac('bootstrap-key', 'sgt-browser-v8|bootstrap-handshake');
  const state = challenge(h.sock, 'bootstrap-key', 'bootstrap');
  await until(() => h.sock.sent.length === 2);
  assert.equal(h.sock.sent[1].mac, hmac(key, `sgt-browser-v8|client|bootstrap|${state.clientNonce}|${state.serverNonce}`));
  h.sock.receive(frame(key, state, 1, { type: 'pair' }));
  h.sock.receive(frame(key, state, 2, { type: 'ready' }));
  h.sock.receive(frame(key, state, 3, { type: 'tabs', id: 7, effectToken: 'test-7', timeoutMs: 10000, action: 'list' }));
  await until(() => h.sock.sent.length === 4);
  assert.equal(h.stored.secretV8, hmac('bootstrap-key', 'sgt-browser-v8|durable'));
  assert.equal(h.sock.sent[2].seq, 1);
  assert.equal(decrypt(key, state, h.sock.sent[2]).type, 'ready_ack');
  assert.equal(h.sock.secretAtSend[2], h.stored.secretV8);
  assert.equal(decrypt(key, state, h.sock.sent[3]).type, 'cdp_dispatch_ready');
  assert.deepEqual(h.sock.sent[3], frame(key, state, 3, decrypt(key, state, h.sock.sent[3]), 'c2s'));
  h.sock.receive(frame(key, state, 4, { type: 'cdp_dispatch_grant', requestId: 7, remainingMs: 5000 }));
  await until(() => h.sock.sent.length === 6);
  assert.equal(decrypt(key, state, h.sock.sent[4]).id, 7);
  assert.equal(decrypt(key, state, h.sock.sent[5]).type, 'effect_terminal');
  h.debuggerEvents.handler({ tabId: 1 }, 'one', { text: 'sensitive content' });
  h.debuggerEvents.handler({ tabId: 1 }, 'two', {});
  await until(() => h.sock.sent.length === 8);
  assert.deepEqual(h.sock.sent.slice(2).map((entry) => entry.seq), [1, 3, 4, 5, 6, 7]);
  assert.deepEqual(h.sock.sent.slice(6).map((entry) => decrypt(key, state, entry).method), ['one', 'two']);
  assert.equal(decrypt(key, state, h.sock.sent[6]).params.text, 'sensitive content');
  assert.ok(!JSON.stringify(h.sock.sent[6]).includes('sensitive content'));
  assert.ok(!h.sock.sent[6].ciphertext.includes(Buffer.from('sensitive content').toString('hex')));
});

test('durable reconnect authenticates and rejects replay and tampering', async () => {
  const h = await harness({ secret: 'durable' });
  const state = challenge(h.sock, 'durable', 'secret');
  await until(() => h.sock.sent.length === 2);
  const ready = frame('durable', state, 1, { type: 'ready' });
  h.sock.receive(ready);
  h.sock.receive(frame('durable', state, 2, { type: 'tabs', id: 1, effectToken: 'durable-1', timeoutMs: 10000, action: 'list' }));
  await until(() => h.sock.sent.length === 4);
  h.sock.receive(frame('durable', state, 3, { type: 'cdp_dispatch_grant', requestId: 1, remainingMs: 5000 }));
  await until(() => h.sock.sent.length === 6);
  h.sock.receive(ready);
  await until(() => h.sock.readyState === 3);
  assert.equal(h.operations.filter(([op]) => op === 'tabs.query').length, 1);

  const other = await harness({ secret: 'durable' });
  const next = challenge(other.sock, 'durable', 'secret');
  await until(() => other.sock.sent.length === 2);
  const tampered = frame('durable', next, 1, { type: 'ready' });
  tampered.ciphertext = (tampered.ciphertext[0] === '0' ? '1' : '0') + tampered.ciphertext.slice(1);
  other.sock.receive(tampered);
  await until(() => other.sock.readyState === 3);
  assert.deepEqual(other.operations, []);
});

test('bootstrap storage failure closes before ready acknowledgement', async () => {
  const h = await harness({ bootstrap: 'bootstrap-key', storageError: true });
  const key = hmac('bootstrap-key', 'sgt-browser-v8|bootstrap-handshake');
  const state = challenge(h.sock, 'bootstrap-key', 'bootstrap');
  await until(() => h.sock.sent.length === 2);
  h.sock.receive(frame(key, state, 1, { type: 'pair' }));
  h.sock.receive(frame(key, state, 2, { type: 'ready' }));
  await until(() => h.sock.readyState === 3);
  assert.equal(h.stored.secretV8, '');
  assert.equal(h.sock.sent.length, 2);
});

test('a legacy raw-bootstrap challenge signature cannot authenticate a host', async () => {
  const h = await harness({ bootstrap: 'bootstrap-key' });
  const nonce = 'ab'.repeat(32), client = h.sock.sent[0].clientNonce;
  const mac = hmac('bootstrap-key', `sgt-browser-v8|server|bootstrap|${client}|${nonce}`);
  challenge(h.sock, 'bootstrap-key', 'bootstrap', nonce, mac);
  await until(() => h.sock.readyState === 3);
  assert.equal(h.sock.sent.length, 1);
  assert.deepEqual(h.operations, []);
});

test('legacy bootstrap and durable keys never authorize protocol 8', async () => {
  for (const mode of ['bootstrap', 'secret']) {
    const h = await harness({ legacyBootstrap: 'legacy-boot', legacySecret: 'legacy-secret' });
    assert.equal(h.sock.sent[0].hasSecret, false);
    assert.equal(h.sock.sent[0].hasBootstrap, false);
    challenge(h.sock, mode === 'bootstrap' ? 'legacy-boot' : 'legacy-secret', mode);
    await until(() => h.sock.readyState === 3);
    assert.equal(h.sock.sent.length, 1);
    assert.equal(h.stored.secret, 'legacy-secret');
    assert.equal(h.stored.secretV8, '');
  }
});

test('reconnect uses a fresh nonce and retains debugger cleanup until authenticated', async () => {
  const h = await harness({ secret: 'durable' });
  const first = challenge(h.sock, 'durable', 'secret');
  await until(() => h.sock.sent.length === 2);
  h.sock.receive(frame('durable', first, 1, { type: 'ready' }));
  h.sock.receive(frame('durable', first, 2, { type: 'tabs', id: 1, effectToken: 'reconnect-1', timeoutMs: 10000, action: 'list' }));
  await until(() => h.sock.sent.length === 4);
  h.sock.receive(frame('durable', first, 3, { type: 'cdp_dispatch_grant', requestId: 1, remainingMs: 5000 }));
  await until(() => h.sock.sent.length === 6);
  h.sock.receive(frame('durable', first, 4, { type: 'effect_terminal_ack', effectToken: 'reconnect-1' }));
  await sleep();
  h.sock.close();
  const detach = h.timers.find((timer) => timer.ms === 15000);
  assert.ok(detach && !detach.cleared);

  h.startup.handler();
  await until(() => h.sockets.length === 2);
  const second = h.sockets[1]; second.open();
  assert.notEqual(second.sent[0].clientNonce, first.clientNonce);
  assert.equal(detach.cleared, false);
  const secondState = challenge(second, 'durable', 'secret');
  await until(() => second.sent.length === 2);
  second.receive(frame('durable', secondState, 1, { type: 'ready' }));
  second.receive(frame('durable', secondState, 2, { type: 'tabs', id: 2, effectToken: 'reconnect-2', timeoutMs: 10000, action: 'list' }));
  await until(() => second.sent.length === 4);
  second.receive(frame('durable', secondState, 3, { type: 'cdp_dispatch_grant', requestId: 2, remainingMs: 5000 }));
  await until(() => second.sent.length === 6);
  assert.equal(detach.cleared, true);

  second.receive(frame('durable', first, 4, { type: 'tabs', id: 3, action: 'list' }));
  await until(() => second.readyState === 3);
  assert.equal(h.operations.filter(([op]) => op === 'tabs.query').length, 2);
});

test('rejected reconnects keep debugger cleanup and increase backoff until authentication', async () => {
  const h = await harness({ secret: 'durable' });
  h.sock.close();
  const detach = h.timers.find(timer => timer.ms === 15000);
  assert.ok(detach && !detach.cleared);

  for (const delay of [2000, 4000, 8000]) {
    const reconnect = h.timers.find(timer => timer.ms === delay && !timer.cleared);
    assert.ok(reconnect, `missing ${delay}ms reconnect`);
    reconnect.cleared = true;
    reconnect.fn();
    const previousCount = h.sockets.length;
    await until(() => h.sockets.length > previousCount);
    const socket = h.sockets.at(-1);
    socket.open();
    assert.equal(detach.cleared, false);
    socket.close();
  }

  h.startup.handler();
  await until(() => h.sockets.length === 5);
  const socket = h.sockets.at(-1);
  socket.open();
  const state = challenge(socket, 'durable', 'secret');
  await until(() => socket.sent.length === 2);
  socket.receive(frame('durable', state, 1, { type: 'ready' }));
  await until(() => detach.cleared);
  const oldReconnectCount = h.timers.filter(timer => timer.ms === 2000).length;
  socket.close();
  assert.equal(h.timers.filter(timer => timer.ms === 2000).length, oldReconnectCount + 1);
});

test('client-to-server ciphertext is never accepted in the opposite direction', async () => {
  const h = await harness({ secret: 'durable' });
  const state = challenge(h.sock, 'durable', 'secret');
  await until(() => h.sock.sent.length === 2);
  h.sock.receive(frame('durable', state, 1, { type: 'ready' }));
  h.sock.receive(frame('durable', state, 2, { type: 'tabs', id: 1, action: 'list' }, 'c2s'));
  await until(() => h.sock.readyState === 3);
  assert.deepEqual(h.operations, []);
});

test('credential changes close the active socket and reconnect with the new key', async () => {
  const h = await harness({ secret: 'old-secret' });
  const state = challenge(h.sock, 'old-secret', 'secret');
  await until(() => h.sock.sent.length === 2);
  h.sock.receive(frame('old-secret', state, 1, { type: 'ready' }));
  await until(() => h.sock.sent.length === 3);
  h.stored.secretV8 = 'new-secret';
  h.messages.handler('reconnect');
  assert.equal(h.sock.readyState, 3);
  h.startup.handler();
  await until(() => h.sockets.length === 2);
  const second = h.sockets[1]; second.open();
  const next = challenge(second, 'new-secret', 'secret');
  await until(() => second.sent.length === 2);
  second.receive(frame('new-secret', next, 1, { type: 'ready' }));
  await until(() => second.sent.length === 3);
  assert.equal(decrypt('new-secret', next, second.sent[2]).type, 'ready_ack');

  h.stored.secretV8 = '';
  h.messages.handler('reconnect');
  assert.equal(second.readyState, 3);
  h.startup.handler();
  await until(() => h.sockets.length === 3);
  const third = h.sockets[2]; third.open();
  assert.equal(third.sent[0].hasSecret, false);
  challenge(third, 'new-secret', 'secret');
  await until(() => third.readyState === 3);
  assert.equal(third.sent.length, 1);
});

test('oversized page-controlled event payloads are omitted before encryption', async () => {
  const h = await harness({ secret: 'durable' });
  const state = challenge(h.sock, 'durable', 'secret');
  await until(() => h.sock.sent.length === 2);
  h.sock.receive(frame('durable', state, 1, { type: 'ready' }));
  await until(() => h.sock.sent.length === 3);
  const huge = 'x'.repeat(5_000_000);
  h.debuggerEvents.handler({ tabId: 4, sessionId: 'child' }, 'Runtime.consoleAPICalled', { args: [{ value: huge }] });
  h.debuggerEvents.handler({ tabId: 4 }, 'Runtime.consoleAPICalled', { args: [{ value: 'small' }] });
  const cyclic = {}; cyclic.self = cyclic;
  h.debuggerEvents.handler({ tabId: 4 }, 'Network.requestWillBeSent', cyclic);
  await until(() => h.sock.sent.length === 6);
  const [omitted, kept, unserializable] = h.sock.sent.slice(3).map(entry => decrypt('durable', state, entry));
  assert.ok(h.sock.sent[3].ciphertext.length < 1024);
  assert.deepEqual({ ...omitted }, { type: 'event', tabId: 4, sessionId: 'child', method: 'Runtime.consoleAPICalled', event_omitted: true });
  assert.equal(kept.params.args[0].value, 'small');
  assert.equal(kept.event_omitted, undefined);
  assert.equal(unserializable.event_omitted, true);
  assert.equal(unserializable.params, undefined);
});

test('a stored port outside the TCP range falls back to the default bridge port', async () => {
  for (const port of [70000, 0, -1, 1.5, '47801', null]) {
    const h = await harness({ sharedStored: { secretV8: 'durable', port } });
    assert.equal(h.sock.url, 'ws://127.0.0.1:47800', String(port));
  }
  const custom = await harness({ sharedStored: { secretV8: 'durable', port: 47801 } });
  assert.equal(custom.sock.url, 'ws://127.0.0.1:47801');
});
