import { strict as assert } from 'node:assert';
import { mock, test } from 'node:test';
import { fakeAi, fakeSession, pcmChunk, prompts } from './support/fake-live-music.mjs';

const { LiveMusicHelper } = await import('../utils/LiveMusicHelper.ts');
const { needsNewHelper, retireHelper } = await import('../utils/HelperLifecycle.ts');

const flush = () => new Promise(resolve => setImmediate(resolve));

function quiet(helper) {
  helper.debug = () => {};
  const states = [];
  helper.addEventListener('playback-state-changed', event => states.push(event.detail));
  return states;
}

function startPlay() {
  const ai = fakeAi();
  const helper = new LiveMusicHelper(ai, 'model');
  const states = quiet(helper);
  helper.setWeightedPrompts(prompts);
  const playing = helper.play();
  return { ai, helper, states, playing };
}

async function connectedHelper() {
  const { ai, helper, states, playing } = startPlay();
  const session = fakeSession();
  ai.connections[0].pending.resolve(session);
  await playing;
  return { helper, states, session, callbacks: ai.connections[0].callbacks };
}

test('every audio chunk in a server message is decoded and scheduled in order', async () => {
  const { helper, callbacks } = await connectedHelper();
  const chunks = [pcmChunk(100, 101), { mimeType: 'audio/l16' }, pcmChunk(200), pcmChunk(300, 301, 302)];
  await callbacks.onmessage({ serverContent: { audioChunks: chunks } });
  const started = helper.audioContext.started;
  assert.equal(started.length, 3);
  assert.deepEqual(started.map(({ buffer }) => Math.round(buffer.channels[0][0] * 32768)), [100, 200, 300]);
  for (let index = 1; index < started.length; index++) {
    assert.equal(started[index].when, started[index - 1].when + started[index - 1].buffer.duration);
  }
  helper.dispose();
});

test('stop while connecting closes the late session without playing it', async () => {
  const { ai, helper, states, playing } = startPlay();
  helper.stop();
  const session = fakeSession();
  ai.connections[0].pending.resolve(session);
  await playing;
  assert.deepEqual(session.calls, ['stop']);
  assert.equal(helper.audioContext.resumed, 0);
  assert.equal(states.at(-1), 'stopped');
  helper.dispose();
});

test('pause while connecting keeps the session idle', async () => {
  const { ai, helper, playing } = startPlay();
  helper.pause();
  const session = fakeSession();
  ai.connections[0].pending.resolve(session);
  await playing;
  assert.equal(session.calls.includes('play'), false);
  assert.equal(helper.audioContext.resumed, 0);
  helper.dispose();
});

for (const halt of ['stop', 'pause']) {
  test(`${halt} during initial buffering cancels the buffer timer`, async () => {
    mock.timers.enable({ apis: ['setTimeout'] });
    try {
      const { helper, states, callbacks } = await connectedHelper();
      await callbacks.onmessage({ serverContent: { audioChunks: [pcmChunk(1)] } });
      helper[halt]();
      const settled = states.at(-1);
      mock.timers.tick(2000);
      await flush();
      assert.equal(states.at(-1), settled);
      assert.equal(states.includes('playing'), false);
      helper.dispose();
    } finally {
      mock.timers.reset();
    }
  });
}

test('buffer timer still promotes uninterrupted playback', async () => {
  mock.timers.enable({ apis: ['setTimeout'] });
  try {
    const { helper, states, callbacks } = await connectedHelper();
    await callbacks.onmessage({ serverContent: { audioChunks: [pcmChunk(1)] } });
    mock.timers.tick(2000);
    assert.equal(states.at(-1), 'playing');
    helper.dispose();
  } finally {
    mock.timers.reset();
  }
});

test('settings refresh keeps the helper; replacement retires the playing one', async () => {
  assert.equal(needsNewHelper('key', 'key', true), false);
  assert.equal(needsNewHelper('key', 'other', true), true);
  assert.equal(needsNewHelper(null, 'key', false), true);

  const { helper, states, session, callbacks } = await connectedHelper();
  let analyserStopped = false;
  retireHelper(helper, null, () => assert.fail('no recording'), { stop() { analyserStopped = true; } });
  await flush();
  assert.equal(analyserStopped, true);
  assert.equal(helper.isDisposed, true);
  assert.equal(helper.audioContext.closed, true);
  assert.equal(session.calls.at(-1), 'stop');
  assert.equal(states.at(-1), 'stopped');
  await callbacks.onmessage({ serverContent: { audioChunks: [pcmChunk(1)] } });
  assert.equal(helper.audioContext.started.length, 0);
  await helper.play();
  assert.equal(states.at(-1), 'stopped');
});

test('a recording owner keeps its context open until the recorder finishes', async () => {
  const { helper } = await connectedHelper();
  let recordingStopped = false;
  retireHelper(helper, helper, () => { recordingStopped = true; }, null);
  await flush();
  assert.equal(recordingStopped, true);
  assert.equal(helper.isDisposed, true);
  assert.equal(helper.audioContext.closed, false);
});

test('pause releases the faded output once its fade completes', async () => {
  mock.timers.enable({ apis: ['setTimeout'] });
  try {
    const { helper } = await connectedHelper();
    const faded = helper.outputNode;
    helper.pause();
    assert.notEqual(helper.outputNode, faded);
    assert.equal(faded.disconnected, false, 'the fade must still be audible to the speaker');
    mock.timers.tick(200);
    assert.equal(faded.disconnected, true);
    helper.dispose();
  } finally {
    mock.timers.reset();
  }
});
