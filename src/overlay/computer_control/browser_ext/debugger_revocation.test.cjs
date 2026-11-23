const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const source = fs.readFileSync(require('node:path').join(__dirname, 'debugger_revocation.js'), 'utf8');
const create = vm.runInNewContext(`${source}\ncreateDebuggerRevocations`);
const key = 'debuggerRevocationsV1';
const code = 'ERR_BROWSER_DEBUGGER_REVOKED';
function storage(initial = {}) {
  const data = structuredClone(initial);
  return { data, async get() { return structuredClone(data); }, async set(value) { Object.assign(data, structuredClone(value)); } };
}

test('revocation survives a worker restart and resume clears only its tab', async () => {
  const store = storage();
  const first = create(store);
  await first.prepare(7);
  const persist = first.revoke(7);
  assert.throws(() => first.assertAllowed(7), { code });
  await persist;
  await first.revoke(8);
  const next = create(store);
  await assert.rejects(next.prepare(7), { code });
  await next.prepare(9);
  await next.resume(7);
  await next.prepare(7);
  await assert.rejects(next.prepare(8), { code });
});

test('an event during storage loading cannot be overwritten by the old snapshot', async () => {
  let release;
  const store = storage();
  store.get = () => new Promise(resolve => { release = resolve; });
  const state = create(store);
  const persisted = state.revoke(7);
  release({ [key]: { version:1, tabs:[8] } });
  await persisted;
  for (const id of [7,8]) await assert.rejects(state.prepare(id), { code });
  assert.deepEqual([...store.data[key].tabs].sort(), [7,8]);
});

test('a fresh cancellation racing a resume remains revoked on disk and in memory', async () => {
  const store = storage({ [key]: { version:1, tabs:[7] } });
  const state = create(store);
  await assert.rejects(state.prepare(7), { code });
  const original = store.set;
  let entered, release;
  const started = new Promise(resolve => { entered = resolve; });
  store.set = async value => {
    store.set = original;
    entered();
    await new Promise(resolve => { release = resolve; });
    await original(value);
  };
  const resumed = state.resume(7);
  await started;
  const persisted = state.revoke(7);
  release();
  await resumed; await persisted;
  await assert.rejects(state.prepare(7), { code });
  assert.deepEqual([...store.data[key].tabs], [7]);
});

test('storage failures and malformed records fail closed', async () => {
  for (const record of [{version:2,tabs:[]}, {version:1,tabs:['7']}, {version:1,tabs:Array(257).fill(7)}]) {
    await assert.rejects(create(storage({[key]:record})).prepare(9), {code:'ERR_BROWSER_DEBUGGER_REVOCATION_STORAGE'});
  }
  const store = storage();
  const state = create(store);
  await state.prepare(9);
  store.set = async () => { throw new Error('unavailable'); };
  await assert.rejects(state.revoke(7));
  await assert.rejects(state.prepare(9), {code:'ERR_BROWSER_DEBUGGER_REVOCATION_STORAGE'});
});

test('closing a tab retires only its persisted revocation', async () => {
  const store = storage({[key]:{version:1,tabs:[7,8]}});
  const state = create(store);
  await assert.rejects(state.prepare(7), {code});
  await state.forget(7);
  await state.prepare(7);
  await assert.rejects(state.prepare(8), {code});
});

test('overflow fails closed without publishing or accumulating excess tab IDs', async () => {
  const tabs = Array.from({length:256},(_,index) => index + 1);
  const store = storage({[key]:{version:1,tabs}});
  const state = create(store);
  await assert.rejects(state.prepare(1),{code});
  for (let id=257;id<300;id++) await assert.rejects(state.revoke(id),{code:'ERR_BROWSER_DEBUGGER_REVOCATION_STORAGE'});
  assert.equal(store.data[key].tabs.length,256);
  await assert.rejects(state.prepare(999),{code:'ERR_BROWSER_DEBUGGER_REVOCATION_STORAGE'});
});

test('a new browser session retires restored tab IDs but keeps cancellations made in it', async () => {
  const store = storage({[key]:{version:1,tabs:[7,8]}});
  const state = create(store);
  await assert.rejects(state.prepare(7), {code});
  const original = store.set;
  let entered, release;
  const started = new Promise(resolve => { entered = resolve; });
  store.set = async value => {
    store.set = original;
    entered();
    await new Promise(resolve => { release = resolve; });
    await original(value);
  };
  const retired = state.retirePreviousBrowserSession();
  await started;
  const persisted = state.revoke(8);
  release();
  await retired; await persisted;
  await state.prepare(7);
  await assert.rejects(state.prepare(8), {code});
  assert.deepEqual([...store.data[key].tabs], [8]);
  const next = create(store);
  await next.prepare(7);
  await assert.rejects(next.prepare(8), {code});
});

test('retiring a previous browser session fails closed when storage is unavailable', async () => {
  const store = storage({[key]:{version:1,tabs:[7]}});
  const state = create(store);
  await assert.rejects(state.prepare(7), {code});
  store.set = async () => { throw new Error('unavailable'); };
  await assert.rejects(state.retirePreviousBrowserSession());
  assert.throws(() => state.assertAllowed(7));
});
