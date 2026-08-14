import { afterEach, describe, expect, it, vi } from "vitest";
import {
  clearAudioWaveformCache,
  getAudioWaveform,
  MAX_WAVEFORM_CACHE_ENTRIES,
  MAX_WAVEFORM_CONCURRENT_REQUESTS,
} from "@/lib/audioWaveform";

type TestWindow = Window & {
  invoke?: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
};

describe("audio waveform cache", () => {
  afterEach(() => {
    clearAudioWaveformCache();
    delete (window as TestWindow).invoke;
  });

  it("buckets nearby target bin counts to avoid resize cache churn", async () => {
    const invoke = vi.fn(async (_cmd: string, args?: Record<string, unknown>) => ({
      bins: Array.from({ length: Number(args?.targetBins ?? 0) }, () => ({
        min: -0.1,
        max: 0.1,
      })),
      sourceDurationSec: 10,
    }));
    (window as TestWindow).invoke = invoke as TestWindow["invoke"];

    await getAudioWaveform("C:\\SGT-Test\\waveform-a.wav", 257);
    await getAudioWaveform("C:\\SGT-Test\\waveform-a.wav", 300);

    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith("get_audio_waveform", {
      path: "C:\\SGT-Test\\waveform-a.wav",
      targetBins: 320,
    });
  });

  it("evicts old waveform entries instead of growing for every project", async () => {
    const invoke = vi.fn(async () => ({
      bins: [{ min: -0.1, max: 0.1 }],
      sourceDurationSec: 1,
    }));
    (window as TestWindow).invoke = invoke as TestWindow["invoke"];

    for (let index = 0; index <= MAX_WAVEFORM_CACHE_ENTRIES; index += 1) {
      await getAudioWaveform(`C:\\SGT-Test\\waveform-${index}.wav`, 64);
    }
    await getAudioWaveform("C:\\SGT-Test\\waveform-0.wav", 64);

    expect(invoke).toHaveBeenCalledTimes(MAX_WAVEFORM_CACHE_ENTRIES + 2);
  });

  it("bounds actual waveform IPC concurrency", async () => {
    let active = 0;
    let maxActive = 0;
    const releases: Array<() => void> = [];
    const invoke = vi.fn(async () => {
      active += 1;
      maxActive = Math.max(maxActive, active);
      await new Promise<void>((resolve) => releases.push(resolve));
      active -= 1;
      return { bins: [], sourceDurationSec: 1 };
    });
    (window as TestWindow).invoke = invoke as TestWindow["invoke"];
    const requests = Array.from({ length: 12 }, (_, index) =>
      getAudioWaveform(`C:\\SGT-Test\\concurrent-${index}.wav`, 64),
    );

    await vi.waitFor(() => {
      expect(invoke).toHaveBeenCalledTimes(MAX_WAVEFORM_CONCURRENT_REQUESTS);
    });
    while (releases.length > 0 || invoke.mock.calls.length < requests.length) {
      releases.splice(0).forEach((release) => release());
      await Promise.resolve();
    }
    await Promise.all(requests);

    expect(maxActive).toBeLessThanOrEqual(MAX_WAVEFORM_CONCURRENT_REQUESTS);
    expect(invoke).toHaveBeenCalledTimes(requests.length);
  });
});
