import { afterEach, describe, expect, it, vi } from "vitest";
import { getAudioWaveform } from "@/lib/audioWaveform";

type TestWindow = Window & {
  invoke?: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
};

describe("audio waveform cache", () => {
  afterEach(() => {
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
});
