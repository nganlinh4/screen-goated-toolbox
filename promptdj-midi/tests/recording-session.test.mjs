import { strict as assert } from 'node:assert';
import { test } from 'node:test';

const { startRecordingSession } = await import('../utils/RecordingSession.ts');

function fakeGraph() {
  const connected = new Set();
  const tee = {
    connect: (node) => { connected.add(node); return node; },
    disconnect: (node) => {
      if (!connected.delete(node)) throw new Error('InvalidAccessError: not connected');
    },
  };
  const context = { createMediaStreamDestination: () => ({ stream: { id: 'stream' } }) };
  return { tee, context, connected };
}

function fakeRecorder() {
  return {
    state: 'inactive', mimeType: 'audio/webm', ondataavailable: null, onstop: null,
    start() { this.state = 'recording'; },
    stop() { this.state = 'inactive'; this.onstop?.(); },
  };
}

test('a finished recording disconnects its capture node from the playback graph', async () => {
  const { tee, context, connected } = fakeGraph();
  const recorder = fakeRecorder();
  let stopped;
  const finished = new Promise((resolve) => { stopped = resolve; });
  startRecordingSession({
    tee, context, createRecorder: () => recorder,
    onStopped: async (blob, chunks) => stopped({ size: blob.size, chunks }),
  });
  assert.equal(connected.size, 1);
  recorder.ondataavailable({ data: new Blob(['abc']) });
  recorder.stop();
  assert.deepEqual(await finished, { size: 3, chunks: 1 });
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(connected.size, 0, 'the capture destination keeps processing audio forever otherwise');
});

test('repeated recordings do not accumulate capture nodes', async () => {
  const { tee, context, connected } = fakeGraph();
  for (let index = 0; index < 3; index += 1) {
    const recorder = fakeRecorder();
    startRecordingSession({ tee, context, createRecorder: () => recorder, onStopped: async () => {} });
    recorder.stop();
    await new Promise((resolve) => setImmediate(resolve));
  }
  assert.equal(connected.size, 0);
});

test('a recorder that cannot be created leaves the graph as it was', () => {
  const { tee, context, connected } = fakeGraph();
  assert.throws(() => startRecordingSession({
    tee, context,
    createRecorder: () => { throw new Error('NotSupportedError'); },
    onStopped: async () => {},
  }), /NotSupportedError/);
  assert.equal(connected.size, 0);
});

test('a failed post-processing step still releases the capture node', async () => {
  const { tee, context, connected } = fakeGraph();
  const recorder = fakeRecorder();
  const originalError = console.error;
  console.error = () => {};
  startRecordingSession({
    tee, context, createRecorder: () => recorder,
    onStopped: async () => { throw new Error('decode failed'); },
  });
  try {
    recorder.stop();
    await new Promise((resolve) => setImmediate(resolve));
  } finally {
    console.error = originalError;
  }
  assert.equal(connected.size, 0);
});
