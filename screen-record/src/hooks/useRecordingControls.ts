import { useState, useRef, useEffect, useCallback } from "react";
import { invoke } from "@/lib/ipc";
import {
  DEFAULT_RECORDING_AUDIO_SELECTION,
  normalizeRecordingAudioSelection,
  sanitizeRecordingAudioSelection,
  type RecordingAudioSelection,
} from "@/types/recordingAudio";
import { type RecordingMode, type VideoSegment } from "@/types/video";
import { type MonitorInfo } from "@/hooks/useAppHooks";

export const RECORDING_MODE_KEY = "screen-record-recording-mode-v1";
export const CAPTURE_SOURCE_KEY = "screen-record-capture-source-v1";
export const RECORDING_AUDIO_KEY = "screen-record-recording-audio-v1";

export function getInitialRecordingMode(): RecordingMode {
  try {
    const raw = localStorage.getItem(RECORDING_MODE_KEY);
    if (raw === "withCursor" || raw === "withoutCursor") return raw;
  } catch {
    // ignore
  }
  return "withoutCursor";
}

export function getInitialCaptureSource(): "monitor" | "window" {
  try {
    const raw = localStorage.getItem(CAPTURE_SOURCE_KEY);
    if (raw === "monitor" || raw === "window") return raw;
  } catch {
    // ignore
  }
  return "monitor";
}

export function getInitialRecordingAudioSelection(): RecordingAudioSelection {
  try {
    const raw = localStorage.getItem(RECORDING_AUDIO_KEY);
    if (!raw) return { ...DEFAULT_RECORDING_AUDIO_SELECTION };
    return normalizeRecordingAudioSelection(JSON.parse(raw));
  } catch {
    return { ...DEFAULT_RECORDING_AUDIO_SELECTION };
  }
}

export interface UseRecordingControlsParams {
  monitors: MonitorInfo[];
  getMonitors: () => Promise<MonitorInfo[]>;
  getWindows: () => Promise<unknown>;
  isRecording: boolean;
  startNewRecording: (
    targetId: string,
    recordingMode: RecordingMode,
    targetType: "monitor" | "window",
    fps: number | undefined,
    audioSelection: RecordingAudioSelection,
  ) => Promise<void>;
  setError: (error: string) => void;
  showWindowSelect: boolean;
  setShowWindowSelect: (show: boolean) => void;
  currentProjectId: string | null;
  currentVideo: string | null;
  segment: VideoSegment | null;
  persistCurrentProjectNow: (options?: {
    refreshList?: boolean;
    includeMedia?: boolean;
  }) => Promise<void>;
  setCurrentRecordingMode: (mode: RecordingMode) => void;
  setCurrentRawVideoPath: (path: string) => void;
  setCurrentRawMicAudioPath: (path: string) => void;
  setCurrentRawWebcamVideoPath: (path: string) => void;
  setLastRawSavedPath: (path: string) => void;
  setRawButtonSavedFlash: (flash: boolean) => void;
}

export function useRecordingControls({
  monitors,
  getMonitors,
  getWindows,
  isRecording,
  startNewRecording,
  setError,
  showWindowSelect,
  setShowWindowSelect,
  currentProjectId,
  currentVideo,
  segment,
  persistCurrentProjectNow,
  setCurrentRecordingMode,
  setCurrentRawVideoPath,
  setCurrentRawMicAudioPath,
  setCurrentRawWebcamVideoPath,
  setLastRawSavedPath,
  setRawButtonSavedFlash,
}: UseRecordingControlsParams) {
  const [selectedRecordingMode, setSelectedRecordingMode] =
    useState<RecordingMode>(getInitialRecordingMode);
  const [captureSource, setCaptureSource] = useState<"monitor" | "window">(
    getInitialCaptureSource,
  );
  const [recordingAudioSelection, setRecordingAudioSelection] =
    useState<RecordingAudioSelection>(getInitialRecordingAudioSelection);
  const [isSelectingRecordingAudioApp, setIsSelectingRecordingAudioApp] =
    useState(false);
  const [captureTargetId, setCaptureTargetId] = useState<string>("0");
  const [captureFps, setCaptureFps] = useState<number | null>(() => {
    try {
      const saved = localStorage.getItem("screen-record-capture-fps-v1");
      return saved ? parseInt(saved, 10) : null;
    } catch {
      return null;
    }
  });
  const captureFpsRef = useRef<number | null>(captureFps);
  const pendingWindowRecordingRef = useRef(false);

  // Persist selectedRecordingMode
  useEffect(() => {
    try {
      localStorage.setItem(RECORDING_MODE_KEY, selectedRecordingMode);
    } catch {
      // ignore persistence failures
    }
  }, [selectedRecordingMode]);

  // Persist captureSource
  useEffect(() => {
    try {
      localStorage.setItem(CAPTURE_SOURCE_KEY, captureSource);
    } catch {
      // ignore persistence failures
    }
  }, [captureSource]);

  // Persist recordingAudioSelection
  useEffect(() => {
    try {
      localStorage.setItem(
        RECORDING_AUDIO_KEY,
        JSON.stringify(
          sanitizeRecordingAudioSelection({
            ...recordingAudioSelection,
            selectedDeviceApp: null,
          }),
        ),
      );
    } catch {
      // ignore persistence failures
    }
  }, [recordingAudioSelection]);

  // Persist captureTargetId
  useEffect(() => {
    try {
      localStorage.setItem("screen-record-capture-target-v1", captureTargetId);
    } catch {}
  }, [captureTargetId]);

  // Persist captureFps
  useEffect(() => {
    try {
      if (captureFps === null)
        localStorage.removeItem("screen-record-capture-fps-v1");
      else
        localStorage.setItem(
          "screen-record-capture-fps-v1",
          captureFps.toString(),
        );
    } catch {}
    captureFpsRef.current = captureFps;
  }, [captureFps]);

  // Restore captureTargetId from localStorage on mount
  useEffect(() => {
    try {
      const saved = localStorage.getItem("screen-record-capture-target-v1");
      if (saved) setCaptureTargetId(saved);
    } catch {}
  }, []);

  // Refresh window list while window-select dialog is open
  useEffect(() => {
    if (!showWindowSelect) return;

    let cancelled = false;
    let inFlight = false;

    const refreshWindows = async () => {
      if (inFlight || cancelled) return;
      inFlight = true;
      try {
        await getWindows();
      } catch {
        // noop
      } finally {
        inFlight = false;
      }
    };

    void refreshWindows();
    const timer = window.setInterval(() => {
      void refreshWindows();
    }, 1200);

    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [showWindowSelect, getWindows]);

  const handleSelectMonitorCapture = useCallback(
    (monitorId: string, fps: number | null) => {
      setCaptureSource("monitor");
      setCaptureFps(fps);
      captureFpsRef.current = fps;
      setCaptureTargetId(monitorId);
    },
    [],
  );

  const handleSelectWindowCapture = useCallback((fps: number | null) => {
    setCaptureSource("window");
    setCaptureFps(fps);
    captureFpsRef.current = fps;
    setCaptureTargetId("0");
  }, []);

  const finalizeStartRecording = useCallback(
    async (targetId: string, targetType: "monitor" | "window") => {
      await startNewRecording(
        targetId,
        selectedRecordingMode,
        targetType,
        captureFpsRef.current ?? undefined,
        sanitizeRecordingAudioSelection(recordingAudioSelection),
      );
    },
    [recordingAudioSelection, selectedRecordingMode, startNewRecording],
  );

  const handleToggleRecordingDeviceAudio = useCallback((enabled: boolean) => {
    setRecordingAudioSelection((prev) => ({
      ...prev,
      deviceEnabled: enabled,
      deviceMode: enabled ? prev.deviceMode : "all",
      selectedDeviceApp: enabled ? prev.selectedDeviceApp : null,
    }));
  }, []);

  const handleToggleRecordingMicAudio = useCallback((enabled: boolean) => {
    setRecordingAudioSelection((prev) => ({
      ...prev,
      micEnabled: enabled,
    }));
  }, []);

  const handleSelectAllRecordingDeviceAudio = useCallback(() => {
    setIsSelectingRecordingAudioApp(false);
    setRecordingAudioSelection((prev) => ({
      ...prev,
      deviceEnabled: true,
      deviceMode: "all",
      selectedDeviceApp: null,
    }));
  }, []);

  const handleRequestRecordingAudioAppSelection = useCallback(async () => {
    setRecordingAudioSelection((prev) => ({
      ...prev,
      deviceEnabled: true,
      deviceMode: "app",
      selectedDeviceApp: null,
    }));
    setIsSelectingRecordingAudioApp(true);
    try {
      await invoke("show_recording_audio_app_selector");
    } catch (error) {
      console.error("[RecordingAudio] Failed to open app selector:", error);
      setIsSelectingRecordingAudioApp(false);
      setRecordingAudioSelection((prev) => ({
        ...prev,
        deviceEnabled: true,
        deviceMode: "all",
        selectedDeviceApp: null,
      }));
    }
  }, []);

  const handleSelectWindowForRecording = useCallback(
    async (windowId: string, _captureMethod: "game" | "window") => {
      setShowWindowSelect(false);
      setCaptureTargetId(windowId);
      if (!pendingWindowRecordingRef.current) return;
      pendingWindowRecordingRef.current = false;
      try {
        await finalizeStartRecording(windowId, "window");
      } catch (err) {
        setError(err as string);
      }
    },
    [finalizeStartRecording, setError, setShowWindowSelect],
  );

  const handleStartRecording = useCallback(async () => {
    if (isRecording || pendingWindowRecordingRef.current) return;
    try {
      if (currentProjectId && currentVideo && segment) {
        await persistCurrentProjectNow({
          refreshList: false,
          includeMedia: false,
        });
      }
      setCurrentRecordingMode(selectedRecordingMode);
      if (!currentVideo) {
        setCurrentRawVideoPath("");
        setCurrentRawMicAudioPath("");
        setCurrentRawWebcamVideoPath("");
        setLastRawSavedPath("");
      }
      setRawButtonSavedFlash(false);

      let finalTargetId = captureTargetId;
      if (
        captureSource === "monitor" &&
        (!finalTargetId || finalTargetId === "0")
      ) {
        const monitorList =
          monitors.length > 0 ? monitors : await getMonitors();
        const primary = monitorList.find((m) => m.is_primary) ?? monitorList[0];
        finalTargetId = primary?.id ?? "0";
      }

      if (captureSource === "window") {
        pendingWindowRecordingRef.current = true;
        try {
          await invoke("show_window_selector");
        } catch (err) {
          pendingWindowRecordingRef.current = false;
          throw err;
        }
        return;
      }

      await finalizeStartRecording(finalTargetId, captureSource);
    } catch (err) {
      setError(err as string);
    }
  }, [
    isRecording,
    currentProjectId,
    currentVideo,
    segment,
    persistCurrentProjectNow,
    selectedRecordingMode,
    captureSource,
    captureTargetId,
    monitors,
    getMonitors,
    finalizeStartRecording,
    setError,
    setCurrentRecordingMode,
    setCurrentRawVideoPath,
    setCurrentRawMicAudioPath,
    setCurrentRawWebcamVideoPath,
    setLastRawSavedPath,
    setRawButtonSavedFlash,
  ]);

  // Listen for window selections dispatched by the native overlay via IPC.
  useEffect(() => {
    const handler = (event: Event) => {
      const { windowId } = (event as CustomEvent<{ windowId: string }>).detail;
      handleSelectWindowForRecording(windowId, "window");
    };
    window.addEventListener("external-window-selected", handler);
    return () =>
      window.removeEventListener("external-window-selected", handler);
  }, [handleSelectWindowForRecording]);

  useEffect(() => {
    const handler = () => {
      pendingWindowRecordingRef.current = false;
    };
    window.addEventListener("external-window-selection-cancelled", handler);
    return () =>
      window.removeEventListener(
        "external-window-selection-cancelled",
        handler,
      );
  }, []);

  // recording-audio-app-selected IPC event
  useEffect(() => {
    const handler = (event: Event) => {
      const detail = (
        event as CustomEvent<{ pid: number; appName: string }>
      ).detail;
      if (!detail || typeof detail.pid !== "number") return;

      setIsSelectingRecordingAudioApp(false);
      setRecordingAudioSelection((prev) => ({
        ...prev,
        deviceEnabled: true,
        deviceMode: "app",
        selectedDeviceApp: {
          pid: detail.pid,
          name: detail.appName || `PID ${detail.pid}`,
        },
      }));
    };
    window.addEventListener("external-recording-audio-app-selected", handler);
    return () => {
      window.removeEventListener(
        "external-recording-audio-app-selected",
        handler,
      );
    };
  }, []);

  // recording-audio-app-selection-cancelled IPC event
  useEffect(() => {
    const handler = () => {
      setIsSelectingRecordingAudioApp(false);
      setRecordingAudioSelection((prev) => ({
        ...prev,
        deviceMode: "all",
        selectedDeviceApp: null,
      }));
    };
    window.addEventListener(
      "external-recording-audio-app-selection-cancelled",
      handler,
    );
    return () => {
      window.removeEventListener(
        "external-recording-audio-app-selection-cancelled",
        handler,
      );
    };
  }, []);

  return {
    selectedRecordingMode,
    setSelectedRecordingMode,
    captureSource,
    setCaptureSource,
    captureTargetId,
    setCaptureTargetId,
    captureFps,
    setCaptureFps,
    captureFpsRef,
    recordingAudioSelection,
    setRecordingAudioSelection,
    isSelectingRecordingAudioApp,
    setIsSelectingRecordingAudioApp,
    pendingWindowRecordingRef,
    handleSelectMonitorCapture,
    handleSelectWindowCapture,
    finalizeStartRecording,
    handleToggleRecordingDeviceAudio,
    handleToggleRecordingMicAudio,
    handleSelectAllRecordingDeviceAudio,
    handleRequestRecordingAudioAppSelection,
    handleSelectWindowForRecording,
    handleStartRecording,
  };
}
