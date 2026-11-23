// Renderer readiness: the compositor reports ready only after its single
// bundled font is loaded and the shared controls have initialized.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

async function boot({ faces = ['face'], checked = true, controls = true } = {}) {
  const messages = [];
  const loads = [];
  const classes = new Set();
  let warmed = 0;
  const window = {
    ipc: { postMessage: raw => messages.push(raw.startsWith('{') ? JSON.parse(raw) : raw) },
    __SGT_QUEUE_SOURCE_FIT__: { warmup: () => { warmed++; } },
  };
  if (controls) {
    window.__SGT_BUTTON_SCENE__ = {};
    window.updateWindows = () => {};
  }
  vm.runInNewContext(fs.readFileSync(path.join(__dirname, 'renderer_bootstrap.js'), 'utf8'), {
    window, performance: { now: () => 5 },
    document: {
      documentElement: { classList: { add: name => classes.add(name) } },
      fonts: {
        load: font => { loads.push(font); return Promise.resolve(faces); },
        check: () => checked,
      },
    },
  });
  await new Promise(resolve => setImmediate(resolve));
  return { messages, loads, classes, warmed };
}

test('a loaded font and initialized controls report font_ready then renderer_ready', async () => {
  const run = await boot();
  assert.deepEqual(run.loads, ["400 16px 'Google Sans Flex'"]);
  assert.equal(run.messages.length, 2);
  assert.equal(run.messages[0].type, 'font_ready');
  assert.equal(typeof run.messages[0].duration_ms, 'number');
  assert.equal(run.messages[1], 'renderer_ready');
  assert.ok(run.classes.has('sgt-font-ready'));
  assert.equal(run.warmed, 1);
});

for (const [name, options, error] of [
  ['an empty font set', { faces: [] }, /Google Sans Flex/],
  ['a font that never enters the loaded set', { checked: false }, /Google Sans Flex/],
  ['missing result controls', { controls: false }, /Unified result controls did not initialize/],
]) {
  test(`${name} reports a bootstrap error instead of readiness`, async () => {
    const run = await boot(options);
    assert.equal(run.messages.length, 1);
    assert.equal(run.messages[0].type, 'command_error');
    assert.equal(run.messages[0].command, 'font_bootstrap');
    assert.match(run.messages[0].error, error);
    assert.equal(run.classes.size, 0);
  });
}
