import { strict as assert } from 'node:assert';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { PlaybackVolume } from '../utils/PlaybackVolume.ts';

const fixture = JSON.parse(readFileSync(new URL('../../parity-fixtures/prompt-dj/volume.json', import.meta.url)));

test('speaker volume is independent of the recording tap across restarts', () => {
  const speaker = {};
  const recording = {};
  const settings = [];
  const gain = {
    gain: { value: -1, setTargetAtTime(value, time, smoothing) { settings.push([value, time, smoothing]); } },
    connect(destination) { assert.equal(destination, speaker); },
  };
  const context = { destination: speaker, currentTime: 12, createGain: () => gain };
  const output = new PlaybackVolume(context, fixture.initialVolume);
  assert.equal(gain.gain.value, fixture.initialVolume);
  for (const value of fixture.steps) {
    const connections = [];
    output.setVolume(value);
    output.connect({ connect(destination) { connections.push(destination); } }, recording);
    assert.deepEqual(connections, [gain, recording]);
    assert.deepEqual(settings.at(-1), [value, 12, 0.1]);
  }
  assert.equal(fixture.recordingFollowsSlider, false);
  assert.equal(fixture.previewFollowsSlider, true);
  assert.equal(fixture.otherApplicationAudioChanges, false);
});
