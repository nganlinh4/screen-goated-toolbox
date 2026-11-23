const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { performance } = require('node:perf_hooks');

const admissionSource = fs.readFileSync(path.join(__dirname, 'cdp_admission.js'), 'utf8');
const workerSource = fs.readFileSync(path.join(__dirname, 'sw.js'), 'utf8');
const handler = workerSource.slice(workerSource.indexOf('async function handleRuntime('), workerSource.indexOf('// Resolve the target tab'));
const KEY = 'effectReloadHandoffV11';
const pause = () => new Promise(resolve => setTimeout(resolve, 1));
async function until(predicate) {
  for (let i = 0; i < 1000; i++) { if (predicate()) return; await pause(); }
  assert.fail('timed out waiting for reload state');
}

function storage(shared, hooks = {}) {
  return {
    async get() { return { ...shared }; },
    async set(values) {
      if (hooks.set) await hooks.set(values);
      Object.assign(shared, values);
    },
    async remove(key) { delete shared[key]; },
  };
}

async function worker(shared, hooks = {}) {
  const events = [], replies = [], calls = [];
  const context = { setTimeout, clearTimeout, performance, Promise, console: { warn() {} } };
  vm.createContext(context);
  vm.runInContext(admissionSource, context);
  const api = vm.runInContext('({ createEffectLedger, createCdpAdmission, createReloadHandoff })', context);
  const ledger = api.createEffectLedger(event => events.push(event));
  const handoff = api.createReloadHandoff(storage(shared, hooks), ledger);
  await handoff.ready();
  const admission = api.createCdpAdmission(() => true, async event => { events.push(event); }, ledger);
  context.reloadHandoff = handoff;
  context.chrome = { runtime: { reload() { calls.push('reload'); if (hooks.reload) return hooks.reload(); } } };
  const handleRuntime = vm.runInContext(handler + '\nhandleRuntime', context);
  function start(id, token) {
    const job = admission.accept({ id, effectToken: token, timeoutMs: 10000 });
    assert.ok(job);
    const running = handleRuntime({ id, type: 'runtime', action: 'reload', effectToken: token },
      reply => replies.push(reply), job).finally(() => admission.finish(job));
    return { job, running };
  }
  return { events, replies, calls, ledger, handoff, admission, start };
}

test('restart before grant replays only the persisted reload token until acknowledged', async () => {
  const shared = {};
  const old = await worker(shared);
  const first = old.start(1, 'reload-before-grant');
  await until(() => old.events.some(event => event.type === 'cdp_dispatch_ready'));
  assert.equal(shared[KEY].token, 'reload-before-grant');
  assert.deepEqual(old.calls, []);

  // A new worker represents termination of the old JS context: no grant can
  // be delivered to that context after the restart.
  const next = await worker(shared);
  next.ledger.replay();
  assert.deepEqual(JSON.parse(JSON.stringify(next.events)), [{
    type: 'effect_terminal', effectToken: 'reload-before-grant', effectMayHaveOccurred: true,
  }]);
  await next.handoff.acknowledge('reload-before-grant');
  next.ledger.acknowledge('reload-before-grant');
  assert.equal(shared[KEY], undefined);
  const later = await worker(shared);
  later.ledger.replay();
  assert.deepEqual(later.events, []);
  first.job.revoke();
  await first.running;
});

test('restart during reload invocation retains terminal evidence after lost reply', async () => {
  const shared = {};
  const old = await worker(shared, { reload() { throw new Error('worker terminated'); } });
  const call = old.start(2, 'reload-after-grant');
  await until(() => old.events.some(event => event.type === 'cdp_dispatch_ready'));
  old.admission.control({ type: 'cdp_dispatch_grant', requestId: 2, remainingMs: 5000 });
  await call.running;
  assert.deepEqual(old.calls, ['reload']);
  assert.equal(shared[KEY].token, 'reload-after-grant');
  assert.deepEqual(old.replies, [], 'the old worker cannot acknowledge its own reload');
  assert.equal(old.events.some(event => event.type === 'effect_terminal'), false);
  assert.deepEqual(JSON.parse(JSON.stringify(old.ledger.snapshot().active)), ['reload-after-grant']);

  const next = await worker(shared);
  next.ledger.replay();
  assert.equal(next.events[0].effectToken, 'reload-after-grant');
  assert.equal(next.events[0].effectMayHaveOccurred, true);
  await next.handoff.acknowledge('reload-after-grant');
  next.ledger.acknowledge('reload-after-grant');
});

for (const [label, reload] of [
  ['successful void return', () => undefined],
  ['rejected thenable', () => Promise.reject(new Error('reload rejected'))],
]) {
  test(`${label} stays unresolved until a fresh worker recovers the marker`, async () => {
    const shared = {};
    const old = await worker(shared, { reload });
    const call = old.start(6, `reload-${label.replaceAll(' ', '-')}`);
    await until(() => old.events.some(event => event.type === 'cdp_dispatch_ready'));
    old.admission.control({ type: 'cdp_dispatch_grant', requestId: 6, remainingMs: 5000 });
    await call.running;
    assert.deepEqual(old.calls, ['reload']);
    assert.deepEqual(old.replies, []);
    assert.equal(old.events.some(event => event.type === 'effect_terminal'), false);
    assert.equal(old.ledger.snapshot().active.length, 1);
    assert.equal(shared[KEY].token, `reload-${label.replaceAll(' ', '-')}`);

    const next = await worker(shared);
    next.ledger.replay();
    assert.equal(next.events.length, 1);
    assert.equal(next.events[0].effectToken, shared[KEY].token);
    assert.equal(next.events[0].effectMayHaveOccurred, true);
    await next.handoff.acknowledge(shared[KEY].token);
  });
}

test('failed marker persistence never requests a grant or invokes reload', async () => {
  const shared = {};
  const old = await worker(shared, { async set() { throw new Error('storage unavailable'); } });
  await old.start(3, 'reload-storage-failure').running;
  assert.equal(old.events.some(event => event.type === 'cdp_dispatch_ready'), false);
  assert.equal(old.events.find(event => event.type === 'effect_terminal').effectMayHaveOccurred, false);
  assert.deepEqual(old.calls, []);
  assert.equal(shared[KEY], undefined);
  const next = await worker(shared);
  next.ledger.replay();
  assert.deepEqual(next.events, []);
});

test('late canceled marker write cannot overwrite a newer reload handoff', async () => {
  const shared = {};
  let unblock, firstSet = true, started = false;
  const stalled = new Promise(resolve => { unblock = resolve; });
  const old = await worker(shared, { async set() {
    if (firstSet) { firstSet = false; started = true; await stalled; }
  } });
  const first = old.start(4, 'reload-canceled');
  await until(() => started);
  first.job.revoke();
  await first.running;
  const second = old.start(5, 'reload-next');
  await pause();
  assert.equal(shared[KEY], undefined);
  unblock();
  await until(() => old.events.some(event => event.type === 'cdp_dispatch_ready' && event.requestId === 5));
  assert.equal(shared[KEY].token, 'reload-next');
  old.admission.control({ type: 'cdp_dispatch_grant', requestId: 5, remainingMs: 5000 });
  await second.running;
  assert.deepEqual(old.calls, ['reload']);
  await old.handoff.acknowledge('reload-next');
});
