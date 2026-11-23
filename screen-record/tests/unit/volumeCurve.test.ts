import { afterEach, describe, expect, it, vi } from "vitest";
import { DEFAULT_DEVICE_AUDIO_VOLUME, getDeviceAudioVolumeAtTime } from "@/lib/deviceAudio";
import { DEFAULT_MIC_AUDIO_VOLUME, getMicAudioVolumeAtTime } from "@/lib/micAudio";

describe("shared mic/device volume curve", () => {
  afterEach(() => vi.restoreAllMocks());

  it("keeps each track's default and cosine interpolation", () => {
    expect(getMicAudioVolumeAtTime(1, [])).toBe(DEFAULT_MIC_AUDIO_VOLUME);
    expect(getDeviceAudioVolumeAtTime(1, null)).toBe(DEFAULT_DEVICE_AUDIO_VOLUME);
    const points = [
      { time: 2, volume: 1 },
      { time: 0, volume: 0 },
    ];
    expect(getMicAudioVolumeAtTime(1, points)).toBeCloseTo(0.5);
    expect(getDeviceAudioVolumeAtTime(0.5, points)).toBeCloseTo((1 - Math.cos(Math.PI / 4)) / 2);
    expect(getDeviceAudioVolumeAtTime(-1, points)).toBe(0);
    expect(getMicAudioVolumeAtTime(5, points)).toBe(1);
  });

  it("sorts a point array once instead of on every per-frame lookup", () => {
    const points = [
      { time: 3, volume: 0.2 },
      { time: 1, volume: 0.8 },
    ];
    const sort = vi.spyOn(Array.prototype, "sort");
    for (let frame = 0; frame < 120; frame += 1) {
      getDeviceAudioVolumeAtTime(frame / 60, points);
      getMicAudioVolumeAtTime(frame / 60, points);
    }
    expect(sort).toHaveBeenCalledTimes(1);
  });
});
