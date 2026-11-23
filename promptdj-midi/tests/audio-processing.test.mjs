import { strict as assert } from 'node:assert';
import { afterEach, test } from 'node:test';
import { processAudioBlob, trimSilence } from '../utils/AudioProcessing.ts';

function fakeBuffer(channels, sampleRate) {
  return {
    numberOfChannels: channels.length,
    length: channels[0].length,
    sampleRate,
    getChannelData: (index) => channels[index],
  };
}

function fakeContext(decoded) {
  return {
    decodeAudioData: async () => decoded,
    createBuffer: (numberOfChannels, length, sampleRate) =>
      fakeBuffer(Array.from({ length: numberOfChannels }, () => new Float32Array(length)), sampleRate),
  };
}

const originalAudioContext = globalThis.AudioContext;
afterEach(() => {
  globalThis.AudioContext = originalAudioContext;
});

test('processing a recording never opens another audio context', async () => {
  let opened = 0;
  globalThis.AudioContext = class {
    constructor() {
      opened += 1;
      throw new Error('a new AudioContext holds an output device until it is closed');
    }
  };
  const decoded = fakeBuffer([Float32Array.from([0, 0, 0.5, -0.5, 0.25, 0])], 48000);

  const wav = await processAudioBlob({ arrayBuffer: async () => new ArrayBuffer(8) }, fakeContext(decoded));

  assert.equal(opened, 0);
  assert.equal(wav.type, 'audio/wav');
  // 44-byte header plus three kept mono 16-bit samples.
  assert.equal(wav.size, 44 + 3 * 2);
});

test('trimming keeps every channel between the first and last audible sample', () => {
  const left = Float32Array.from([0, 0.1, 0.9, 0, 0]);
  const right = Float32Array.from([0, 0, 0.3, 0.4, 0]);
  const trimmed = trimSilence(fakeBuffer([left, right], 44100), fakeContext(null));

  assert.equal(trimmed.length, 3);
  assert.equal(trimmed.sampleRate, 44100);
  assert.deepEqual(Array.from(trimmed.getChannelData(0)), [left[1], left[2], left[3]]);
  assert.deepEqual(Array.from(trimmed.getChannelData(1)), [right[1], right[2], right[3]]);
});

test('a silent recording is rejected', () => {
  assert.equal(trimSilence(fakeBuffer([new Float32Array(4)], 48000), fakeContext(null)), null);
});
