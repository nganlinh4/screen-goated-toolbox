import { strict as assert } from 'node:assert';
import { mock, test } from 'node:test';
import { fakeAi } from './support/fake-live-music.mjs';

const { LiveMusicHelper } = await import('../utils/LiveMusicHelper.ts');

const flush = () => new Promise(resolve => setImmediate(resolve));

function weights(weight) {
  return new Map([['p', { promptId: 'p', text: 'Funk', weight, cc: 0, color: '#fff' }]]);
}

function recordingSession() {
  const sent = [];
  const calls = [];
  const record = name => () => { calls.push(name); };
  return {
    sent,
    calls,
    play: record('play'),
    pause: record('pause'),
    stop: record('stop'),
    setWeightedPrompts: async ({ weightedPrompts }) => { sent.push(weightedPrompts.map(p => p.weight)); },
  };
}

async function connect(initialWeight) {
  const ai = fakeAi();
  const helper = new LiveMusicHelper(ai, 'model');
  helper.debug = () => {};
  helper.setWeightedPrompts(weights(initialWeight));
  const playing = helper.play();
  const session = recordingSession();
  ai.connections[0].pending.resolve(session);
  await playing;
  return { helper, session };
}

test('a new session receives the prompts set just before play', async () => {
  const { helper, session } = await connect(0.7);
  assert.deepEqual(session.sent.at(-1), [0.7]);
  assert.ok(session.calls.includes('play'));
  helper.dispose();
});

test('the last weight of a fast knob sweep always reaches the session', async () => {
  mock.timers.enable({ apis: ['setTimeout', 'Date'] });
  try {
    const { helper, session } = await connect(1);
    mock.timers.tick(1000);
    for (const weight of [1.2, 1.5, 1.8, 2]) {
      helper.setWeightedPrompts(weights(weight));
      mock.timers.tick(30);
    }
    mock.timers.tick(500);
    await flush();
    assert.deepEqual(session.sent.at(-1), [2]);
    // Bursts are still coalesced rather than sent once per input event.
    assert.ok(session.sent.length <= 3, `sent ${session.sent.length} updates`);
    helper.dispose();
  } finally {
    mock.timers.reset();
  }
});

test('a stopped helper does not send a coalesced update afterwards', async () => {
  mock.timers.enable({ apis: ['setTimeout', 'Date'] });
  try {
    const { helper, session } = await connect(1);
    helper.setWeightedPrompts(weights(1.5));
    const sentBeforeStop = session.sent.length;
    helper.dispose();
    mock.timers.tick(500);
    await flush();
    assert.equal(session.sent.length, sentBeforeStop);
  } finally {
    mock.timers.reset();
  }
});
