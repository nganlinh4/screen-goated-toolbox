import { useCallback, useEffect, useRef } from "react";
import { invoke } from "@/lib/ipc";
import { recordingHandoff } from "@/lib/recordingHandoff";
import { beginAudioImportShutdown, cancelAudioImportShutdown, hasPendingAudioImport } from '@/lib/audioImportWriteFence';
import { cancelSubtitleShutdownOverride, clearSubtitleShutdownBlocked, consumeSubtitleShutdownOverride,
  hasPendingSubtitleShutdownWork, showSubtitleShutdownBlocked } from './subtitleShutdown';
import { useSettings } from "@/hooks/useSettings";
import { projectManager } from "@/lib/projectManager";
import {
  BackgroundConfig,
  Project,
  ProjectComposition,
  WebcamConfig,
} from "@/types/video";
import { ensureProjectComposition } from "@/lib/projectComposition";
import { getNewRecordingCameraConfig } from "@/lib/recordingDefaults";

export interface UseStopRecordingParams {
  handleStopRecording: () => Promise<{
    mouseData: import("@/types/video").MousePosition[];
    initialSegment: import("@/types/video").VideoSegment;
    videoUrl: string;
    webcamVideoUrl: string | null;
    recordingMode: import("@/types/video").RecordingMode;
    rawVideoPath: string | null;
    rawMicAudioPath: string | null;
    rawWebcamVideoPath: string | null;
    capturedFps: number | null;
    recordingId: string;
    markSaved: () => Promise<void>;
  } | null>;
  backgroundConfig: BackgroundConfig;
  generateThumbnail: () => string | null | undefined;
  projects: {
    setCurrentProjectId: (id: string | null) => void;
    loadProjects: () => Promise<void>;
  };
  rawAutoCopyEnabled: boolean;
  rawSaveDir: string;
  flashRawSavedButton: () => void;
  setShowRawVideoDialog: (show: boolean) => void;
  setShowExportSuccessDialog: (show: boolean) => void;
  requestCloseProjects: () => void;
  setComposition: (c: ProjectComposition | null) => void;
  setCurrentProjectData: (p: Project | null) => void;
  setLoadedClipId: (id: string | null) => void;
  setLastCaptureFps: (fps: number | null) => void;
  setCurrentRecordingMode: (mode: import("@/types/video").RecordingMode) => void;
  setCurrentRawVideoPath: (path: string) => void;
  setCurrentRawMicAudioPath: (path: string) => void;
  setCurrentRawWebcamVideoPath: (path: string) => void;
  setLastRawSavedPath: (path: string) => void;
  setIsRawActionBusy: (busy: boolean) => void;
  setError: (error: string) => void;
  setWebcamConfig: (config: WebcamConfig) => void;
}

export function useStopRecording({
  handleStopRecording,
  backgroundConfig,
  generateThumbnail,
  projects,
  rawAutoCopyEnabled,
  rawSaveDir,
  flashRawSavedButton,
  setShowRawVideoDialog,
  setShowExportSuccessDialog,
  requestCloseProjects,
  setComposition,
  setCurrentProjectData,
  setLoadedClipId,
  setLastCaptureFps,
  setCurrentRecordingMode,
  setCurrentRawVideoPath,
  setCurrentRawMicAudioPath,
  setCurrentRawWebcamVideoPath,
  setLastRawSavedPath,
  setIsRawActionBusy,
  setError,
  setWebcamConfig,
}: UseStopRecordingParams) {
  const { t } = useSettings();
  const savedProjectRef = useRef<{ recordingId: string; project: Project } | null>(null);
  const onStopRecording = useCallback(() => recordingHandoff.stop(async () => {
    setShowRawVideoDialog(false);
    setShowExportSuccessDialog(false);
    const result = await handleStopRecording();
    if (result) {
      setComposition(null);
      setLoadedClipId(null);
      setCurrentProjectData(null);
      projects.setCurrentProjectId(null);
      requestCloseProjects();
      const {
        mouseData,
        initialSegment,
        videoUrl,
        webcamVideoUrl,
        recordingMode,
        rawVideoPath,
        rawMicAudioPath,
        rawWebcamVideoPath,
        capturedFps,
        recordingId,
        markSaved,
      } = result;
      setLastCaptureFps(capturedFps);
      setCurrentRecordingMode(recordingMode);
      setCurrentRawVideoPath(rawVideoPath || "");
      setCurrentRawMicAudioPath(rawMicAudioPath || "");
      setCurrentRawWebcamVideoPath(rawWebcamVideoPath || "");
      setLastRawSavedPath("");
      const nextWebcamConfig = getNewRecordingCameraConfig(initialSegment.webcamAvailable === true);
      setWebcamConfig(nextWebcamConfig);

      if (rawAutoCopyEnabled && rawVideoPath && rawSaveDir) {
        try {
          setIsRawActionBusy(true);
          const saved = await invoke<{ savedPath: string }>(
            "save_raw_video_copy",
            {
              sourcePath: rawVideoPath,
              targetDir: rawSaveDir,
            },
          );
          const autoSavedPath = saved?.savedPath || "";
          if (autoSavedPath) {
            setLastRawSavedPath(autoSavedPath);
            await invoke("copy_video_file_to_clipboard", {
              filePath: autoSavedPath,
            });
            flashRawSavedButton();
          }
        } catch (e) {
          console.error("[RawVideo] Auto-copy after recording failed:", e);
        } finally {
          setIsRawActionBusy(false);
        }
      }

      let videoBlob: Blob | undefined;
      let webcamBlob: Blob | undefined;
      if (!rawVideoPath) {
        const response = await fetch(videoUrl);
        videoBlob = await response.blob();
      }
      if (!rawWebcamVideoPath && webcamVideoUrl) {
        const response = await fetch(webcamVideoUrl);
        webcamBlob = await response.blob();
      }
      const thumbnail = generateThumbnail();
      const project = savedProjectRef.current?.recordingId === recordingId
        ? savedProjectRef.current.project
        : await projectManager.saveProject({
        name: `Recording ${new Date().toLocaleString()}`,
        videoBlob,
        webcamBlob,
        segment: initialSegment,
        backgroundConfig,
        webcamConfig: nextWebcamConfig,
        mousePositions: mouseData,
        thumbnail: thumbnail || undefined,
        duration: initialSegment.trimEnd,
        recordingMode,
        rawVideoPath: rawVideoPath || undefined,
        rawMicAudioPath: rawMicAudioPath || undefined,
        rawWebcamVideoPath: rawWebcamVideoPath || undefined,
      }, recordingId);
      savedProjectRef.current = { recordingId, project };
      await markSaved();
      savedProjectRef.current = null;
      projects.setCurrentProjectId(project.id);
      setCurrentProjectData(project);
      setComposition(ensureProjectComposition(project));
      setLoadedClipId("root");
      try {
        await projects.loadProjects();
      } catch (error) {
        console.error("Failed to refresh saved projects", error);
      }
      return true;
    }
    return false;
  }).then((saved) => {
    window.dispatchEvent(new Event(saved ? "sr-recording-retry-cleared" : "sr-recording-retry-required"));
    return saved;
  }), [
    handleStopRecording,
    backgroundConfig,
    generateThumbnail,
    projects,
    rawAutoCopyEnabled,
    rawSaveDir,
    flashRawSavedButton,
    setShowRawVideoDialog,
    setShowExportSuccessDialog,
    requestCloseProjects,
    setComposition,
    setCurrentProjectData,
    setLoadedClipId,
    setLastCaptureFps,
    setCurrentRecordingMode,
    setCurrentRawVideoPath,
    setCurrentRawMicAudioPath,
    setCurrentRawWebcamVideoPath,
    setLastRawSavedPath,
    setIsRawActionBusy,
    setWebcamConfig,
  ]);

  // Handler registration must outlive ordinary renders. In particular, a
  // render after a positive acknowledgement must not reopen audio admission
  // before the native window has actually closed.
  const shutdownCallbacks = useRef({ onStopRecording, setError, t });
  shutdownCallbacks.current = { onStopRecording, setError, t };
  useEffect(() => {
    let shutdownToken: object | null = null;
    let shutdownRequestId: number | null = null;
    let blockedByAudio = false;
    let blockedBySubtitle = false;
    let disposed = false;
    const prepare = (event: Event) => {
      const { requestId, subtitleOverrideId } = (event as CustomEvent<{ requestId?: number; subtitleOverrideId?: number }>).detail ?? {};
      if (!Number.isSafeInteger(requestId)) return;
      blockedByAudio = false;
      blockedBySubtitle = false;
      const token = beginAudioImportShutdown();
      if (!token) {
        if (subtitleOverrideId) cancelSubtitleShutdownOverride(subtitleOverrideId);
        if (hasPendingAudioImport()) { blockedByAudio = true; shutdownCallbacks.current.setError(shutdownCallbacks.current.t.audioRecoveryShutdownBlocked); }
        void invoke("shutdown_ack", { requestId, saved: false }).catch((error) => console.error("Failed to acknowledge recorder shutdown", error));
        return;
      }
      blockedByAudio = false;
      if (!consumeSubtitleShutdownOverride(subtitleOverrideId) && hasPendingSubtitleShutdownWork()) {
        blockedBySubtitle = true;
        cancelAudioImportShutdown(token);
        void invoke("shutdown_ack", { requestId, saved: false }).catch((error) => console.error("Failed to acknowledge recorder shutdown", error));
        return;
      }
      blockedBySubtitle = false;
      clearSubtitleShutdownBlocked();
      shutdownToken = token;
      shutdownRequestId = requestId!;
      void recordingHandoff.prepare(() => shutdownCallbacks.current.onStopRecording()).then((saved) => {
        if (shutdownToken !== token) return;
        if (!saved) {
          cancelAudioImportShutdown(token);
          shutdownToken = null;
          shutdownRequestId = null;
          if (disposed) window.removeEventListener("sr-cancel-shutdown", cancel);
        }
        return invoke("shutdown_ack", { requestId, saved });
      }).catch((error) => console.error("Failed to acknowledge recorder shutdown", error));
    };
    const cancel = (event: Event) => {
      const requestId = (event as CustomEvent<{ requestId?: number }>).detail?.requestId;
      if (!Number.isSafeInteger(requestId) || requestId !== shutdownRequestId) return;
      recordingHandoff.cancelPrepare();
      cancelAudioImportShutdown(shutdownToken);
      shutdownToken = null;
      shutdownRequestId = null;
      if (disposed) window.removeEventListener("sr-cancel-shutdown", cancel);
    };
    const failed = (event: Event) => {
      const overrideId = (event as CustomEvent<{ subtitleOverrideId?: number }>).detail?.subtitleOverrideId;
      if (overrideId) cancelSubtitleShutdownOverride(overrideId);
      const { setError, t } = shutdownCallbacks.current;
      if (blockedBySubtitle) { showSubtitleShutdownBlocked(); setError(t.subtitleShutdownBlocked); }
      else setError(blockedByAudio || hasPendingAudioImport() ? t.audioRecoveryShutdownBlocked : t.recorderShutdownFailed);
    };
    const recover = () => { void shutdownCallbacks.current.onStopRecording(); };
    (window as Window & { __srShutdownReady?: boolean }).__srShutdownReady = true;
    window.addEventListener("sr-prepare-shutdown", prepare);
    window.addEventListener("sr-cancel-shutdown", cancel);
    window.addEventListener("sr-shutdown-failed", failed);
    window.addEventListener("sr-recover-recording", recover);
    return () => {
      disposed = true;
      (window as Window & { __srShutdownReady?: boolean }).__srShutdownReady = false;
      window.removeEventListener("sr-prepare-shutdown", prepare);
      // An in-flight prepare still owns admission after an editor remount.
      // Keep its one-shot cancel listener until native cancellation or window
      // destruction; otherwise a positive ACK can be overtaken by a new import.
      if (!shutdownToken) window.removeEventListener("sr-cancel-shutdown", cancel);
      window.removeEventListener("sr-shutdown-failed", failed);
      window.removeEventListener("sr-recover-recording", recover);
    };
  }, []);

  return { onStopRecording };
}
