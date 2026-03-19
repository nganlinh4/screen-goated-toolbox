import { useCallback } from "react";
import { invoke } from "@/lib/ipc";
import { projectManager } from "@/lib/projectManager";
import {
  BackgroundConfig,
  Project,
  ProjectComposition,
  WebcamConfig,
} from "@/types/video";
import { ensureProjectComposition } from "@/lib/projectComposition";
import { cloneWebcamConfig, DEFAULT_WEBCAM_CONFIG } from "@/lib/webcam";

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
  setWebcamConfig,
}: UseStopRecordingParams) {
  const onStopRecording = useCallback(async () => {
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
      } = result;
      setLastCaptureFps(capturedFps);
      setCurrentRecordingMode(recordingMode);
      setCurrentRawVideoPath(rawVideoPath || "");
      setCurrentRawMicAudioPath(rawMicAudioPath || "");
      setCurrentRawWebcamVideoPath(rawWebcamVideoPath || "");
      setLastRawSavedPath("");
      const nextWebcamConfig = cloneWebcamConfig({
        ...DEFAULT_WEBCAM_CONFIG,
        visible: initialSegment.webcamAvailable !== false,
      });
      setWebcamConfig(nextWebcamConfig);

      let autoSavedPath = "";
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
          autoSavedPath = saved?.savedPath || "";
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
      const project = await projectManager.saveProject({
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
      });
      projects.setCurrentProjectId(project.id);
      setCurrentProjectData(project);
      setComposition(ensureProjectComposition(project));
      setLoadedClipId("root");
      await projects.loadProjects();
    }
  }, [
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

  return { onStopRecording };
}
