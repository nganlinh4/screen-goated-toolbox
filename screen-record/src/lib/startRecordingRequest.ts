import type { RecordingMode } from "@/types/video";
import type { RecordingAudioSelection } from "@/types/recordingAudio";

export interface StartRecordingInput {
  targetId: string;
  targetType: "monitor" | "window";
  recordingMode: RecordingMode;
  fps: number | undefined;
  audioSelection: RecordingAudioSelection;
}

/**
 * Builds the `start_recording` IPC payload. Keystrokes are always captured
 * while a recording runs, whatever the keystroke overlay mode is, so the
 * overlay can be turned on later for an existing recording.
 */
export function buildStartRecordingRequest(input: StartRecordingInput) {
  const { audioSelection } = input;
  return {
    targetId: input.targetId,
    targetType: input.targetType,
    includeCursor: input.recordingMode === "withCursor",
    fps: input.fps ?? null,
    deviceAudioEnabled: audioSelection.deviceEnabled,
    deviceAudioMode: audioSelection.deviceMode,
    deviceAudioAppPid: audioSelection.selectedDeviceApp?.pid ?? null,
    micEnabled: audioSelection.micEnabled,
    webcamEnabled: true,
  };
}
