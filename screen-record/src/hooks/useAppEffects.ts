import { useEffect, useRef, type MutableRefObject, type RefObject } from "react";
import { BackgroundConfig, VideoSegment, ProjectComposition } from "@/types/video";
import { LAST_BG_CONFIG_KEY } from "@/lib/appUtils";
import { saveCropPref } from "@/hooks/useVideoState";
import type { PersistOptions } from "@/hooks/useSequenceComposition";

const AUTOSAVE_DEBOUNCE_MS = 3000;
const AUTOSAVE_BUSY_RETRY_MS = 1000;

export interface UseAppEffectsParams {
  // Segment ref sync
  segment: VideoSegment | null;
  segmentRef: MutableRefObject<VideoSegment | null>;
  // Background config persistence
  backgroundConfig: BackgroundConfig;
  // Canvas mode/size persistence
  currentProjectId: string | null;
  currentVideo: string | null;
  persistRef: MutableRefObject<((opts?: PersistOptions) => Promise<void>) | null>;
  // Toggle recording listener
  isRecording: boolean;
  showHotkeyDialog: boolean;
  onStopRecording: () => void;
  handleStartRecording: () => void;
  // Auto-save
  mousePositions: unknown[];
  composition: ProjectComposition | null;
  currentRecordingMode: string;
  currentRawVideoPath: string;
  duration: number;
  videoRef: RefObject<HTMLVideoElement | null>;
  isProcessing: boolean;
}

export function useAppEffects({
  segment,
  segmentRef,
  backgroundConfig,
  currentProjectId,
  currentVideo,
  persistRef,
  isRecording,
  showHotkeyDialog,
  onStopRecording,
  handleStartRecording,
  mousePositions,
  composition,
  currentRecordingMode,
  currentRawVideoPath,
  duration,
  videoRef,
  isProcessing,
}: UseAppEffectsParams) {
  // Read when a deferred autosave fires, so it sees the current export/recording state.
  const busyRef = useRef({ isProcessing, isRecording });
  busyRef.current = { isProcessing, isRecording };

  // Keep segmentRef current so event handlers always read latest segment
  useEffect(() => {
    segmentRef.current = segment;
  }, [segment, segmentRef]);

  // Persist crop preference so newly recorded/imported videos inherit the last crop.
  useEffect(() => {
    if (!segment) return;
    saveCropPref(segment.crop);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    segment?.crop?.x,
    segment?.crop?.y,
    segment?.crop?.width,
    segment?.crop?.height,
    segment,
  ]);

  // Persist last-used background config so new projects inherit previous project settings.
  useEffect(() => {
    try {
      localStorage.setItem(
        LAST_BG_CONFIG_KEY,
        JSON.stringify(backgroundConfig),
      );
    } catch {
      // ignore persistence failures
    }
  }, [backgroundConfig]);

  // Persist canvas mode/size changes quickly so reopening projects can't
  // resurrect stale custom-canvas settings from an older autosave.
  useEffect(() => {
    if (!currentProjectId || !segment) return;
    if (!currentVideo) return;
    const timer = setTimeout(() => {
      void persistRef.current?.({ refreshList: false, includeMedia: false });
    }, 500);
    return () => clearTimeout(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    currentProjectId,
    currentVideo,
    backgroundConfig.canvasMode,
    backgroundConfig.canvasWidth,
    backgroundConfig.canvasHeight,
  ]);

  // Toggle recording via IPC event
  useEffect(() => {
    const handleToggle = () => {
      if (showHotkeyDialog) return;
      if (isRecording) onStopRecording();
      else handleStartRecording();
    };
    window.addEventListener("toggle-recording", handleToggle);
    return () => window.removeEventListener("toggle-recording", handleToggle);
  }, [isRecording, showHotkeyDialog, onStopRecording, handleStartRecording]);

  // Auto-save — debounced, deferred (not dropped) during playback/export/recording to avoid jank
  useEffect(() => {
    if (!currentProjectId || !segment) return;
    if (!currentVideo) return;
    let timer: ReturnType<typeof setTimeout>;
    const attemptSave = () => {
      const isBusy = (videoRef.current && !videoRef.current.paused) ||
        busyRef.current.isProcessing || busyRef.current.isRecording;
      if (isBusy) {
        // The edit is still unsaved; nothing re-arms this effect until the next edit.
        timer = setTimeout(attemptSave, AUTOSAVE_BUSY_RETRY_MS);
        return;
      }
      void persistRef.current?.({ refreshList: true, includeMedia: true });
    };
    timer = setTimeout(attemptSave, AUTOSAVE_DEBOUNCE_MS);
    return () => clearTimeout(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    segment,
    backgroundConfig,
    mousePositions,
    // Composition-only edits like sequence mode/selection still need persistence.
    composition,
    currentRecordingMode,
    currentRawVideoPath,
    duration,
    currentProjectId,
    currentVideo,
  ]);

  // Last-chance metadata flush for edits that are visible in memory but still
  // waiting behind the debounced autosave when the WebView is hidden/closed.
  useEffect(() => {
    if (!currentProjectId || !segment || !currentVideo) return;

    const flushEditorMetadata = () => {
      void persistRef.current?.({ refreshList: false, includeMedia: false });
    };
    const handleVisibilityChange = () => {
      if (document.visibilityState === "hidden") {
        flushEditorMetadata();
      }
    };

    window.addEventListener("pagehide", flushEditorMetadata);
    document.addEventListener("visibilitychange", handleVisibilityChange);
    return () => {
      window.removeEventListener("pagehide", flushEditorMetadata);
      document.removeEventListener("visibilitychange", handleVisibilityChange);
    };
  }, [currentProjectId, currentVideo, segment, composition, persistRef]);
}
