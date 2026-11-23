import { describe, expect, it } from "vitest";
import { buildStartRecordingRequest } from "@/lib/startRecordingRequest";
import { DEFAULT_RECORDING_AUDIO_SELECTION } from "@/types/recordingAudio";

const request = () =>
  buildStartRecordingRequest({
    targetId: "0",
    targetType: "monitor",
    recordingMode: "withCursor",
    fps: undefined,
    audioSelection: DEFAULT_RECORDING_AUDIO_SELECTION,
  });

describe("buildStartRecordingRequest", () => {
  it("never asks the host to opt in or out of keystroke capture", () => {
    // The host always captures keystrokes; the overlay mode only affects
    // how the recording is shown, so it is not part of the request.
    expect(request()).not.toHaveProperty("captureKeystrokes");
  });

  it("keeps the existing capture fields", () => {
    expect(request()).toEqual({
      targetId: "0",
      targetType: "monitor",
      includeCursor: true,
      fps: null,
      deviceAudioEnabled: true,
      deviceAudioMode: "all",
      deviceAudioAppPid: null,
      micEnabled: false,
      webcamEnabled: true,
    });
  });
});
