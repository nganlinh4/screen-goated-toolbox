import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { installAudioMeter, normalizedMeterLevel } from "./audio-meter.ts";

const fixture = JSON.parse(readFileSync(new URL("../../parity-fixtures/translation-gummy/meter-delivery.json", import.meta.url), "utf8"));

test("meter values match the shared finite range and stopped contract", () => {
  for (const entry of fixture.normalizationCases) {
    const value = entry.valueKind === "nan" ? NaN : entry.valueKind === "infinity" ? Infinity : entry.value;
    assert.equal(normalizedMeterLevel(entry.running, value), entry.expected, entry.name);
  }
});

test("meter bursts update only the existing payload level without reading history or settings", () => {
  const payload = {
    isRunning: true,
    audioLevel: 0,
    get transcripts(): never { throw new Error("meter read history"); },
    get draft(): never { throw new Error("meter read settings"); },
  };
  const target: { __TG_SET_AUDIO_LEVEL?: (value: number) => void } = {};
  installAudioMeter(target, () => payload);
  for (let index = 0; index < fixture.delivery.burstSamples; index++) {
    target.__TG_SET_AUDIO_LEVEL!(index / fixture.delivery.burstSamples);
  }
  assert.equal(payload.audioLevel, 0.999);
  payload.isRunning = false;
  target.__TG_SET_AUDIO_LEVEL!(0.8);
  assert.equal(payload.audioLevel, 0);
});

test("meter reception before initial state is harmless and later uses the current payload", () => {
  const target: { __TG_SET_AUDIO_LEVEL?: (value: number) => void } = {};
  let payload: { isRunning: boolean; audioLevel: number } | null = null;
  installAudioMeter(target, () => payload);
  target.__TG_SET_AUDIO_LEVEL!(0.5);
  payload = { isRunning: true, audioLevel: 0 };
  target.__TG_SET_AUDIO_LEVEL!(0.7);
  assert.equal(payload.audioLevel, 0.7);
});
