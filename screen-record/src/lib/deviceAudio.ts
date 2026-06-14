import type { DeviceAudioPoint } from "@/types/video";
import { clamp } from "@/lib/mathUtils";

export const MIN_DEVICE_AUDIO_VOLUME = 0;
export const MAX_DEVICE_AUDIO_VOLUME = 1;
export const DEFAULT_DEVICE_AUDIO_VOLUME = 1;
export const IMPLICIT_AUDIO_EDGE_FADE_SEC = 0.12;

export function clampDeviceAudioVolume(volume: number) {
  return clamp(
    Number.isFinite(volume) ? volume : DEFAULT_DEVICE_AUDIO_VOLUME,
    MIN_DEVICE_AUDIO_VOLUME,
    MAX_DEVICE_AUDIO_VOLUME,
  );
}

export function buildFlatDeviceAudioPoints(
  duration: number,
  volume: number = DEFAULT_DEVICE_AUDIO_VOLUME,
): DeviceAudioPoint[] {
  const clampedDuration = Math.max(0, duration);
  const clampedVolume = clampDeviceAudioVolume(volume);
  return [
    { time: 0, volume: clampedVolume },
    { time: clampedDuration, volume: clampedVolume },
  ];
}

export function getDeviceAudioVolumeAtTime(
  time: number,
  points: DeviceAudioPoint[] | undefined | null,
): number {
  if (!points || points.length === 0) return DEFAULT_DEVICE_AUDIO_VOLUME;

  const sorted = [...points].sort((a, b) => a.time - b.time);
  const idx = sorted.findIndex((point) => point.time >= time);
  if (idx === -1) {
    return clampDeviceAudioVolume(sorted[sorted.length - 1].volume);
  }
  if (idx === 0) {
    return clampDeviceAudioVolume(sorted[0].volume);
  }

  const left = sorted[idx - 1];
  const right = sorted[idx];
  const ratio = clamp(
    (time - left.time) / Math.max(0.0001, right.time - left.time),
    0,
    1,
  );
  const cosT = (1 - Math.cos(ratio * Math.PI)) / 2;
  return clampDeviceAudioVolume(
    left.volume + (right.volume - left.volume) * cosT,
  );
}

export function getImplicitAudioEdgeFadeMultiplier(
  localTime: number,
  duration: number,
  fadeSec: number = IMPLICIT_AUDIO_EDGE_FADE_SEC,
): number {
  if (!Number.isFinite(localTime) || !Number.isFinite(duration) || duration <= 0) return 0;
  const fade = Math.max(0, Math.min(fadeSec, duration / 2));
  if (fade <= 0) return 1;
  if (localTime <= 0 || localTime >= duration) return 0;
  const fadeIn = localTime < fade
    ? (1 - Math.cos((localTime / fade) * Math.PI)) / 2
    : 1;
  const tailTime = duration - localTime;
  const fadeOut = tailTime < fade
    ? (1 - Math.cos((tailTime / fade) * Math.PI)) / 2
    : 1;
  return clamp(fadeIn * fadeOut, 0, 1);
}

export function normalizeDeviceAudioPoints(
  points: DeviceAudioPoint[] | undefined | null,
  duration: number,
  fallbackVolume: number = DEFAULT_DEVICE_AUDIO_VOLUME,
): DeviceAudioPoint[] {
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
      volume: clampDeviceAudioVolume(point.volume),
    }))
    .sort((a, b) => a.time - b.time);

  if (prepared.length === 0) {
    return buildFlatDeviceAudioPoints(clampedDuration, fallbackVolume);
  }

  if (prepared.length === 1) {
    return buildFlatDeviceAudioPoints(clampedDuration, prepared[0].volume);
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
