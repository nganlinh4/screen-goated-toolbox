import type { MicAudioPoint } from "@/types/video";
import { clamp } from "@/lib/mathUtils";
import { getCurveVolumeAtTime } from "@/lib/volumeCurve";

export const MIN_MIC_AUDIO_VOLUME = 0;
export const MAX_MIC_AUDIO_VOLUME = 1;
export const DEFAULT_MIC_AUDIO_VOLUME = 0;

export function clampMicAudioVolume(volume: number) {
  return clamp(
    Number.isFinite(volume) ? volume : DEFAULT_MIC_AUDIO_VOLUME,
    MIN_MIC_AUDIO_VOLUME,
    MAX_MIC_AUDIO_VOLUME,
  );
}

export function buildFlatMicAudioPoints(
  duration: number,
  volume: number = DEFAULT_MIC_AUDIO_VOLUME,
): MicAudioPoint[] {
  const clampedDuration = Math.max(0, duration);
  const clampedVolume = clampMicAudioVolume(volume);
  return [
    { time: 0, volume: clampedVolume },
    { time: clampedDuration, volume: clampedVolume },
  ];
}

export function getMicAudioVolumeAtTime(
  time: number,
  points: MicAudioPoint[] | undefined | null,
): number {
  return getCurveVolumeAtTime(time, points, DEFAULT_MIC_AUDIO_VOLUME, clampMicAudioVolume);
}

export function normalizeMicAudioPoints(
  points: MicAudioPoint[] | undefined | null,
  duration: number,
  fallbackVolume: number = DEFAULT_MIC_AUDIO_VOLUME,
): MicAudioPoint[] {
  const clampedDuration = Math.max(0, duration);
  const prepared = (points ?? [])
    .filter(
      (point) =>
        point &&
        Number.isFinite(point.time) &&
        Number.isFinite(point.volume),
    )
    .map((point) => ({
      time: clamp(point.time, 0, clampedDuration),
      volume: clampMicAudioVolume(point.volume),
    }))
    .sort((a, b) => a.time - b.time);

  if (prepared.length === 0) {
    return buildFlatMicAudioPoints(clampedDuration, fallbackVolume);
  }

  if (prepared.length === 1) {
    return buildFlatMicAudioPoints(clampedDuration, prepared[0].volume);
  }

  const normalized = [...prepared];
  if (normalized[0].time > 0) {
    normalized.unshift({ time: 0, volume: normalized[0].volume });
  } else {
    normalized[0] = { ...normalized[0], time: 0 };
  }

  const lastIndex = normalized.length - 1;
  if (normalized[lastIndex].time < clampedDuration) {
    normalized.push({
      time: clampedDuration,
      volume: normalized[lastIndex].volume,
    });
  } else {
    normalized[lastIndex] = {
      ...normalized[lastIndex],
      time: clampedDuration,
    };
  }

  return normalized;
}
