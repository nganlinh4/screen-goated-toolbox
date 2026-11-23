const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const root = path.resolve(__dirname, '../../..');
const fixture = JSON.parse(fs.readFileSync(path.join(root, 'parity-fixtures/live-translate/model-selector-updates.json'), 'utf8'));
const logic = fs.readFileSync(path.join(__dirname, 'js_logic.rs'), 'utf8');
const start = logic.indexOf('function switchModel(modelName)');
const end = logic.indexOf('window.switchModel = switchModel;', start);
assert.ok(start >= 0 && end > start);
const switchSource = logic.slice(start, end + 'window.switchModel = switchModel;'.length)
  .replaceAll('{{', '{').replaceAll('}}', '}');

function select(options, initial) {
  let selected = initial;
  return {
    get value() { return selected; },
    set value(next) { selected = options.includes(next) ? next : ''; },
  };
}

for (const test of fixture.translationCases) {
  const control = select(test.options, test.initial);
  const messages = [];
  const context = {
    window: { realtimePostMessage: value => messages.push(value) },
    document: { getElementById: id => id === 'translation-model-select' ? control : null },
  };
  vm.runInNewContext(switchSource, context);
  context.window.switchModel(test.hostModel);
  assert.equal(control.value, test.selected, test.name);
  assert.equal(messages.length, test.userMessages, test.name);
}

const tail = fs.readFileSync(path.join(__dirname, 'js_main/tail.rs'), 'utf8');
const settingsStart = tail.indexOf('window.updateSettings = function(settings)');
const settingsEnd = tail.indexOf('// Handle resize', settingsStart);
assert.ok(settingsStart >= 0 && settingsEnd > settingsStart);
const restored = fixture.downloadCancellation.selectedModel;
const transcription = select(['downloaded', restored], 'downloaded');
const calls = [];
const context = {
  window: { realtimePostMessage: value => calls.push(['message', value]) },
  document: { getElementById: id => id === 'transcription-model-select' ? transcription : null },
  micBtn: null, deviceBtn: null, langSelect: null, transLangSelect: null,
  updateTransLangSelectState: model => calls.push(['language', model]),
  isS2sTranscriptionModel: model => model === restored,
  applyS2sMode: (s2s, model) => calls.push(['mode', s2s, model]),
  currentFontSize: 16, content: { style: {} }, minContentHeight: 0,
};
vm.runInNewContext(tail.slice(settingsStart, settingsEnd), context);
context.window.updateSettings({ transcriptionModel: restored });
assert.equal(transcription.value, restored);
assert.deepEqual(calls, [['language', restored], ['mode', true, restored]]);
