import type { TranslationGummyState } from "./types";

type MeterPayload = Pick<TranslationGummyState, "audioLevel" | "isRunning">;
type MeterTarget = { __TG_SET_AUDIO_LEVEL?: (level: number) => void };

export function normalizedMeterLevel(running: boolean, level: number): number {
  return running && Number.isFinite(level) ? Math.max(0, Math.min(1, level)) : 0;
}

export function installAudioMeter(target: MeterTarget, current: () => MeterPayload | null): void {
  target.__TG_SET_AUDIO_LEVEL = (level) => {
    const payload = current();
    if (payload) payload.audioLevel = normalizedMeterLevel(payload.isRunning, level);
  };
}
