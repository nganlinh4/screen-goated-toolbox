import { useCallback, useRef, type MutableRefObject, type RefObject } from "react";
import {
  BackgroundConfig,
  MousePosition,
  Project,
  ProjectComposition,
  VideoSegment,
  RecordingMode,
  WebcamConfig,
} from "@/types/video";
import { useProjectPersistence } from "@/hooks/useProjectPersistence";
import { useRecordingControls } from "@/hooks/useRecordingControls";
import { useProjectActions } from "@/hooks/useProjectActions";
import { useStopRecording } from "@/hooks/useStopRecording";
import type { PersistOptions } from "@/hooks/useSequenceComposition";
import type { ClipMediaAssets } from "@/hooks/useClipMediaCache";
import type { MonitorInfo } from "@/hooks/useAppHooks";
import type { toPreviewRectSnapshot } from "@/lib/appUtils";

export interface UseProjectLifecycleParams {
  // Shared refs
  persistRef: MutableRefObject<((opts?: PersistOptions) => Promise<void>) | null>;
  isProjectTransitionRef: MutableRefObject<boolean>;
  isSwitchingCompositionClipRef: MutableRefObject<boolean>;
  canvasRef: RefObject<HTMLCanvasElement | null>;
  previewContainerRef: MutableRefObject<HTMLDivElement | null>;
  restoreImageRef: MutableRefObject<string | null>;
  projectsPreviewTargetSnapshotRef: MutableRefObject<{
    stageRect: ReturnType<typeof toPreviewRectSnapshot>;
    canvasRect: ReturnType<typeof toPreviewRectSnapshot>;
  } | null>;
  // useProjectPersistence params
  currentProjectId: string | null;
  projects: {
    projects: Project[];
    loadProjects: () => Promise<void>;
    setCurrentProjectId: (id: string | null) => void;
  };
  currentVideo: string | null;
  currentAudio: string | null;
  currentMicAudio: string | null;
  currentWebcamVideo: string | null;
  loadedClipId: string | null;
  currentProjectData: Project | null;
  segment: VideoSegment | null;
  composition: ProjectComposition | null;
  backgroundConfig: BackgroundConfig;
  mousePositions: MousePosition[];
  generateThumbnail: () => string | null | undefined;
  duration: number;
  currentRecordingMode: RecordingMode;
  currentRawVideoPath: string;
  currentRawMicAudioPath: string;
  currentRawWebcamVideoPath: string;
  webcamConfig: WebcamConfig;
  loadClipAssets: (
    projectId: string,
    clipId: string,
    projectData?: Project | null,
    composition?: ProjectComposition | null,
  ) => Promise<ClipMediaAssets | null>;
  setComposition: (c: ProjectComposition | null) => void;
  setCurrentProjectData: (p: Project | null) => void;
  // useRecordingControls params
  monitors: MonitorInfo[];
  getMonitors: () => Promise<MonitorInfo[]>;
  getWindows: () => Promise<unknown>;
  isRecording: boolean;
  startNewRecording: (
    targetId: string,
    recordingMode: RecordingMode,
    targetType: "monitor" | "window",
    fps: number | undefined,
    audioSelection: import("@/types/recordingAudio").RecordingAudioSelection,
  ) => Promise<void>;
  setError: (error: string) => void;
  showWindowSelect: boolean;
  setShowWindowSelect: (show: boolean) => void;
  setCurrentRecordingMode: (mode: RecordingMode) => void;
  setCurrentRawVideoPath: (path: string) => void;
  setCurrentRawMicAudioPath: (path: string) => void;
  setCurrentRawWebcamVideoPath: (path: string) => void;
  setLastRawSavedPath: (path: string) => void;
  setRawButtonSavedFlash: (flash: boolean) => void;
  // useProjectActions params
  projectsDialog: {
    showProjectsDialog: boolean;
    setShowProjectsDialog: (show: boolean) => void;
    handleLoadProject: (id: string) => Promise<void>;
  };
  isProjectInteractionShieldVisible: boolean;
  projectInteractionShieldReleaseRef: MutableRefObject<(() => void) | null>;
  projectInteractionBlockCleanupRef: MutableRefObject<(() => void) | null>;
  beginProjectInteractionShield: () => void;
  abortEditorInteractions: () => void;
  setIsProjectInteractionShieldVisible: (v: boolean) => void;
  armProjectInteractionShieldRelease: () => void;
  clearClipMediaCaches: (opts: {
    preserveVideoUrl: string | null;
    preserveAudioUrl: string | null;
    preserveMicAudioUrl: string | null;
    preserveWebcamVideoUrl: string | null;
  }) => void;
  clipExportSourcePathCacheRef: MutableRefObject<Map<string, string>>;
  clipExportWebcamPathCacheRef: MutableRefObject<Map<string, string | null>>;
  clipLoadRequestSeqRef: MutableRefObject<number>;
  loadClipMediaIntoEditor: (
    projectId: string,
    clipId: string,
    project: Project,
    composition: ProjectComposition,
  ) => Promise<void>;
  setLoadedClipId: (id: string | null) => void;
  applyLoadedBackgroundConfig: (config: BackgroundConfig) => void;
  spreadAnimationTimerRef: MutableRefObject<ReturnType<typeof setTimeout> | null>;
  setSpreadFromClipId: (id: string | null) => void;
  setLastCaptureFps: (fps: number | null) => void;
  setWebcamConfig: (config: WebcamConfig) => void;
  // useStopRecording params
  handleStopRecording: () => Promise<{
    mouseData: MousePosition[];
    initialSegment: VideoSegment;
    videoUrl: string;
    webcamVideoUrl: string | null;
    recordingMode: RecordingMode;
    rawVideoPath: string | null;
    rawMicAudioPath: string | null;
    rawWebcamVideoPath: string | null;
    capturedFps: number | null;
  } | null>;
  rawAutoCopyEnabled: boolean;
  rawSaveDir: string;
  flashRawSavedButton: () => void;
  setShowRawVideoDialog: (show: boolean) => void;
  setShowExportSuccessDialog: (show: boolean) => void;
  setIsRawActionBusy: (busy: boolean) => void;
}

export function useProjectLifecycle({
  persistRef,
  isProjectTransitionRef,
  isSwitchingCompositionClipRef,
  canvasRef,
  previewContainerRef,
  restoreImageRef,
  projectsPreviewTargetSnapshotRef,
  currentProjectId,
  projects,
  currentVideo,
  currentAudio,
  currentMicAudio,
  currentWebcamVideo,
  loadedClipId,
  currentProjectData,
  segment,
  composition,
  backgroundConfig,
  mousePositions,
  generateThumbnail,
  duration,
  currentRecordingMode,
  currentRawVideoPath,
  currentRawMicAudioPath,
  currentRawWebcamVideoPath,
  webcamConfig,
  loadClipAssets,
  setComposition,
  setCurrentProjectData,
  monitors,
  getMonitors,
  getWindows,
  isRecording,
  startNewRecording,
  setError,
  showWindowSelect,
  setShowWindowSelect,
  setCurrentRecordingMode,
  setCurrentRawVideoPath,
  setCurrentRawMicAudioPath,
  setCurrentRawWebcamVideoPath,
  setLastRawSavedPath,
  setRawButtonSavedFlash,
  projectsDialog,
  isProjectInteractionShieldVisible,
  projectInteractionShieldReleaseRef,
  projectInteractionBlockCleanupRef,
  beginProjectInteractionShield,
  abortEditorInteractions,
  setIsProjectInteractionShieldVisible,
  armProjectInteractionShieldRelease,
  clearClipMediaCaches,
  clipExportSourcePathCacheRef,
  clipExportWebcamPathCacheRef,
  clipLoadRequestSeqRef,
  loadClipMediaIntoEditor,
  setLoadedClipId,
  applyLoadedBackgroundConfig,
  spreadAnimationTimerRef,
  setSpreadFromClipId,
  setLastCaptureFps,
  setWebcamConfig,
  handleStopRecording,
  rawAutoCopyEnabled,
  rawSaveDir,
  flashRawSavedButton,
  setShowRawVideoDialog,
  setShowExportSuccessDialog,
  setIsRawActionBusy,
}: UseProjectLifecycleParams) {
  const { persistCurrentProjectNow, debugProject, logProjectSwitch } =
    useProjectPersistence({
      currentProjectId,
      projects: { projects: projects.projects, loadProjects: projects.loadProjects },
      currentVideo,
      currentAudio,
      currentMicAudio,
      currentWebcamVideo,
      loadedClipId,
      currentProjectData,
      segment,
      composition,
      backgroundConfig,
      mousePositions,
      generateThumbnail,
      duration,
      currentRecordingMode,
      currentRawVideoPath,
      currentRawMicAudioPath,
      currentRawWebcamVideoPath,
      webcamConfig,
      canvasRef,
      isProjectTransitionRef,
      isSwitchingCompositionClipRef,
      loadClipAssets,
      setComposition,
    });
  persistRef.current = persistCurrentProjectNow;

  const recordingPersistRefInternal = useRef<
    (options?: { refreshList?: boolean; includeMedia?: boolean }) => Promise<void>
  >(null!);
  recordingPersistRefInternal.current = persistCurrentProjectNow;
  const stableRecordingPersist = useCallback(
    (opts?: { refreshList?: boolean; includeMedia?: boolean }) =>
      recordingPersistRefInternal.current(opts),
    [],
  );

  const recordingControls = useRecordingControls({
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
    persistCurrentProjectNow: stableRecordingPersist,
    setCurrentRecordingMode,
    setCurrentRawVideoPath,
    setCurrentRawMicAudioPath,
    setCurrentRawWebcamVideoPath,
    setLastRawSavedPath,
    setRawButtonSavedFlash,
  });

  const {
    onProjectLoaded,
    handleLoadProjectFromGrid,
    requestCloseProjects,
    handleToggleProjects,
  } = useProjectActions({
    projects: {
      currentProjectId,
      showProjectsDialog: projectsDialog.showProjectsDialog,
      setShowProjectsDialog: projectsDialog.setShowProjectsDialog,
      handleLoadProject: projectsDialog.handleLoadProject,
    },
    isProjectInteractionShieldVisible,
    isProjectTransitionRef,
    projectInteractionShieldReleaseRef,
    projectInteractionBlockCleanupRef,
    beginProjectInteractionShield,
    abortEditorInteractions,
    setIsProjectInteractionShieldVisible,
    armProjectInteractionShieldRelease,
    clearClipMediaCaches,
    clipExportSourcePathCacheRef,
    clipExportWebcamPathCacheRef,
    isSwitchingCompositionClipRef,
    clipLoadRequestSeqRef,
    loadClipMediaIntoEditor,
    setLoadedClipId,
    currentVideo,
    currentAudio,
    currentMicAudio,
    currentWebcamVideo,
    backgroundConfig,
    segment,
    currentProjectData,
    setCurrentProjectData,
    setCurrentRawMicAudioPath,
    setCurrentRawWebcamVideoPath,
    setWebcamConfig,
    setComposition,
    applyLoadedBackgroundConfig,
    spreadAnimationTimerRef,
    setSpreadFromClipId,
    setLastCaptureFps,
    canvasRef,
    previewContainerRef,
    restoreImageRef,
    projectsPreviewTargetSnapshotRef,
    debugProject,
    logProjectSwitch,
    persistCurrentProjectNow,
  });

  const { onStopRecording } = useStopRecording({
    handleStopRecording,
    backgroundConfig,
    generateThumbnail,
    projects: {
      setCurrentProjectId: projects.setCurrentProjectId,
      loadProjects: projects.loadProjects,
    },
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
  });

  return {
    // From useProjectActions
    onProjectLoaded,
    handleLoadProjectFromGrid,
    requestCloseProjects,
    handleToggleProjects,
    // From useStopRecording
    onStopRecording,
    // From useRecordingControls
    ...recordingControls,
  };
}
