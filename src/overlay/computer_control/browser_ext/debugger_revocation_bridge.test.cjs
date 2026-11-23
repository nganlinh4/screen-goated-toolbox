const test = require('node:test');
const assert = require('node:assert/strict');
const { harness, challenge, frame, decrypt, until } = require('./bridge_harness.cjs');

async function connected(options = {}) {
  const h = await harness({ secret:'durable', ...options });
  const state = challenge(h.sock, 'durable', 'secret');
  await until(() => h.sock.sent.length === 2);
  h.sock.receive(frame('durable', state, 1, {type:'ready'}));
  await until(() => h.sock.sent.length === 3);
  let sequence = 1;
  return Object.assign(h, {
    send(body) { h.sock.receive(frame('durable', state, ++sequence, body)); },
    bodies() { return h.sock.sent.filter(item => item.type === 'secure').map(item => decrypt('durable', state, item)); },
  });
}
const call = (id, tabId = 12) => ({type:'cdp',id,tabId,effectToken:`revocation-${id}`,timeoutMs:10000,method:'Input.dispatchMouseEvent'});

test('user Cancel blocks reattachment across worker restart and requires the popup to resume', async () => {
  const h = await connected();
  h.debuggerDetach.handler({tabId:12}, 'canceled_by_user');
  await until(() => h.stored.debuggerRevocationsV1?.tabs.includes(12));
  await until(() => h.bodies().some(body => body.method === 'SGT.debuggerDetached'));
  const next = await connected({sharedStored:h.stored});
  next.send(call(70));
  await until(() => next.bodies().some(body => body.id === 70));
  assert.equal(next.bodies().find(body => body.id === 70).code, 'ERR_BROWSER_DEBUGGER_REVOKED');
  assert.equal(next.operations.some(op => op[0] === 'attach'), false);
  const message = {type:'resume-debugging',tabId:12};
  next.messages.handler(message, {id:'extension-id',url:'https://example.com/'}, () => assert.fail('untrusted resume'));
  assert.deepEqual([...next.stored.debuggerRevocationsV1.tabs],[12]);
  const resumed = await new Promise(resolve => {
    assert.equal(next.messages.handler(message, {id:'extension-id',url:'chrome-extension://extension-id/popup.html'}, resolve),true);
  });
  assert.equal(resumed.ok,true);
  next.send(call(71));
  await until(() => next.bodies().some(body => body.type === 'cdp_dispatch_ready' && body.requestId === 71));
  next.send({type:'cdp_dispatch_grant',requestId:71,remainingMs:5000});
  await until(() => next.bodies().some(body => body.id === 71));
  assert.equal(next.bodies().find(body => body.id === 71).ok,true);
});

test('Cancel while awaiting the host grant prevents the command effect', async () => {
  const h = await connected();
  h.send(call(72));
  await until(() => h.bodies().some(body => body.type === 'cdp_dispatch_ready'));
  h.debuggerDetach.handler({tabId:12}, 'canceled_by_user');
  h.send({type:'cdp_dispatch_grant',requestId:72,remainingMs:5000});
  await until(() => h.bodies().some(body => body.id === 72));
  assert.equal(h.bodies().find(body => body.id === 72).code,'ERR_BROWSER_DEBUGGER_REVOKED');
  assert.equal(h.operations.some(op => op[0] === 'cdp' && op[2] === 'Input.dispatchMouseEvent'),false);
});

test('Cancel during attachment retires a late attachment without starting domains or input', async () => {
  let release;
  const blocked = new Promise(resolve => { release = resolve; });
  const h = await connected({attachHook:() => blocked});
  h.send(call(73));
  await until(() => h.operations.some(op => op[0] === 'attach'));
  h.debuggerDetach.handler({tabId:12},'canceled_by_user');
  release();
  await until(() => h.bodies().some(body => body.id === 73));
  assert.equal(h.bodies().find(body => body.id === 73).code,'ERR_BROWSER_DEBUGGER_REVOKED');
  assert.equal(h.operations.some(op => op[0] === 'detach'),true);
  assert.equal(h.operations.some(op => op[0] === 'cdp'),false);
});

test('revoked tabs cannot be navigated while unrelated tabs remain usable', async () => {
  const h = await connected();
  h.debuggerDetach.handler({tabId:12},'canceled_by_user');
  h.send({type:'tabs',action:'navigate',tabId:12,url:'https://example.com/',id:74,effectToken:'revocation-74',timeoutMs:10000});
  await until(() => h.bodies().some(body => body.id === 74));
  assert.equal(h.bodies().find(body => body.id === 74).code,'ERR_BROWSER_DEBUGGER_REVOKED');
  assert.equal(h.operations.some(op => op[0] === 'tabs.update'),false);
  h.send(call(75,13));
  await until(() => h.bodies().some(body => body.type === 'cdp_dispatch_ready' && body.requestId === 75));
  h.send({type:'cdp_dispatch_grant',requestId:75,remainingMs:5000});
  await until(() => h.bodies().some(body => body.id === 75));
  assert.equal(h.bodies().find(body => body.id === 75).ok,true);
});

test('browser startup retires revocations restored from an earlier browser session', async () => {
  const stored = {secretV8:'durable', debuggerRevocationsV1:{version:1,tabs:[12]}};
  const h = await connected({sharedStored:stored});
  h.startup.handler();
  await until(() => h.stored.debuggerRevocationsV1.tabs.length === 0);
  h.send(call(80));
  await until(() => h.bodies().some(body => body.type === 'cdp_dispatch_ready' && body.requestId === 80));
  h.send({type:'cdp_dispatch_grant',requestId:80,remainingMs:5000});
  await until(() => h.bodies().some(body => body.id === 80));
  assert.equal(h.bodies().find(body => body.id === 80).ok, true);
});
