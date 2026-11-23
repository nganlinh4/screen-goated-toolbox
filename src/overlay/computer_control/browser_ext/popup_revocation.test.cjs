const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const source = fs.readFileSync(require('node:path').join(__dirname,'popup.js'),'utf8');

async function harness() {
  const elements = new Map();
  const messages = [];
  const saved = {secretV8:'paired', debuggerRevocationsV1:{version:1,tabs:[12,13]}};
  let active = 12;
  const activation = {isActive:true};
  const get = id => {
    if (!elements.has(id)) elements.set(id,{ value:'', addEventListener(name,fn) { this[name] = fn; } });
    return elements.get(id);
  };
  vm.runInNewContext(source, {
    document:{getElementById:get}, navigator:{userActivation:activation},
    chrome:{
      storage:{local:{async get() {return saved;}, async set() {}, async remove() {}}},
      tabs:{async query() {return [{id:active}];}},
      runtime:{async sendMessage(message) {messages.push(message); saved.debuggerRevocationsV1.tabs = saved.debuggerRevocationsV1.tabs.filter(id => id !== message.tabId); return {ok:true};}},
    },
  });
  await new Promise(resolve => setImmediate(resolve));
  return { get, messages, activation, active(id) {active=id;} };
}

test('only a trusted active gesture can request resume from the popup', async () => {
  const h = await harness();
  assert.equal(h.get('resume').disabled,false);
  await h.get('resume').click({isTrusted:false});
  h.activation.isActive=false;
  await h.get('resume').click({isTrusted:true});
  assert.equal(h.messages.length,0);
  h.activation.isActive=true;
  await h.get('resume').click({isTrusted:true});
  assert.equal(h.messages.length,1);
  assert.equal(h.messages[0].tabId,12);
  assert.equal(h.get('resume').disabled,true);
});

test('active-tab drift cannot resume a different tab than the displayed choice', async () => {
  const h = await harness();
  h.active(13);
  await h.get('resume').click({isTrusted:true});
  assert.equal(h.messages.length,0);
  await h.get('resume').click({isTrusted:true});
  assert.equal(h.messages[0].tabId,13);
});
