export type RecordingDeviceAudioMode = "all" | "app";

export interface RecordingAudioAppSelection {
  pid: number;
  name: string;
}

export interface RecordingAudioSelection {
  deviceEnabled: boolean;
  micEnabled: boolean;
  deviceMode: RecordingDeviceAudioMode;
  selectedDeviceApp: RecordingAudioAppSelection | null;
}

export const DEFAULT_RECORDING_AUDIO_SELECTION: RecordingAudioSelection = {
  deviceEnabled: true,
  micEnabled: false,
  deviceMode: "all",
  selectedDeviceApp: null,
};

export function normalizeRecordingAudioSelection(
  value: unknown,
): RecordingAudioSelection {
  if (!value || typeof value !== "object") {
    return { ...DEFAULT_RECORDING_AUDIO_SELECTION };
  }

  const candidate = value as Partial<RecordingAudioSelection>;
  const selectedDeviceApp =
    candidate.selectedDeviceApp &&
    typeof candidate.selectedDeviceApp === "object" &&
    typeof candidate.selectedDeviceApp.pid === "number" &&
    Number.isFinite(candidate.selectedDeviceApp.pid) &&
    candidate.selectedDeviceApp.pid > 0 &&
    typeof candidate.selectedDeviceApp.name === "string" &&
    candidate.selectedDeviceApp.name.trim().length > 0
      ? {
          pid: Math.trunc(candidate.selectedDeviceApp.pid),
          name: candidate.selectedDeviceApp.name.trim(),
        }
      : null;

  const requestedMode = candidate.deviceMode === "app" ? "app" : "all";
  const deviceMode =
    requestedMode === "app" && selectedDeviceApp ? "app" : "all";

  return {
    deviceEnabled:
      typeof candidate.deviceEnabled === "boolean"
        ? candidate.deviceEnabled
        : DEFAULT_RECORDING_AUDIO_SELECTION.deviceEnabled,
    micEnabled:
      typeof candidate.micEnabled === "boolean"
        ? candidate.micEnabled
        : DEFAULT_RECORDING_AUDIO_SELECTION.micEnabled,
    deviceMode,
    selectedDeviceApp,
  };
}

export function sanitizeRecordingAudioSelection(
  selection: RecordingAudioSelection,
): RecordingAudioSelection {
  return normalizeRecordingAudioSelection(selection);
}
