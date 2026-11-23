const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const test = require('node:test');
const assert = require('node:assert/strict');

function harness() {
  const source = fs.readFileSync(path.join(__dirname, 'document.rs'), 'utf8');
  const script = source.split('<script>')[1].split('</script>')[0]
    .replaceAll('{{', '{').replaceAll('}}', '}')
    .replace('{transcription}', '""').replace('{translation}', '""');
  const events = {}, messages = [];
  const frames = Object.fromEntries(['transcription', 'translation'].map(role => [role, {
    style: {}, classList: {toggle() {}, add() {}}, addEventListener() {}, contentWindow: {eval() {}}
  }]));
  const window = {devicePixelRatio: 1, ipc: {postMessage: text => messages.push(JSON.parse(text))},
    addEventListener: (name, callback) => { events[name] = callback; }};
  vm.runInNewContext(script, {window, document: {getElementById: id => frames[id.replace('-card', '')]},
    requestAnimationFrame() {}});
  return {window, events, messages};
}

function layout() {
  return {transcription: {x:10,y:20,width:400,height:200,visible:true},
    translation: {x:500,y:20,width:400,height:200,visible:true}};
}

test('session changes reject old drag, resize and end events before local effects', () => {
  const h = harness(), first = layout();
  h.window.applyRealtimeLayout(first, 0, '7');
  h.window.realtimePostMessage('transcription', 'interactionStart');
  h.window.realtimePostMessage('transcription', 'cardDragMove:5,3');
  assert.equal(first.transcription.x, 15);
  const next = layout();
  h.window.applyRealtimeLayout(next, 0, '8');
  const before = h.messages.length;
  h.window.realtimePostMessage('transcription', 'cardDragMove:99,99');
  h.window.realtimePostMessage('transcription', 'saveResize');
  h.window.realtimePostMessage('transcription', 'interactionEnd');
  assert.equal(next.transcription.x, 10);
  assert.equal(h.messages.length, before);
  h.window.realtimePostMessage('transcription', 'interactionStart');
  h.window.realtimePostMessage('transcription', 'resize:5,3');
  assert.equal(next.transcription.width, 405);
  assert.equal(h.messages.at(-1).session_id, '8');
});

test('old text-focus completion is rejected and session IDs retain full precision', () => {
  const h = harness(), exact = '18446744073709551615';
  h.window.applyRealtimeLayout(layout(), 0, '3');
  h.window.realtimePostMessage('translation', 'textInputStart');
  h.window.applyRealtimeLayout(layout(), 0, exact);
  const before = h.messages.length;
  h.window.realtimePostMessage('translation', 'textInputEnd');
  assert.equal(h.messages.length, before);
  h.window.realtimePostMessage('translation', 'textInputStart');
  h.window.realtimePostMessage('translation', 'textInputEnd');
  assert.equal(h.messages.at(-1).session_id, exact);
});
