import { useMediaPipelineRecovery } from "@/hooks/useMediaPipelineRecovery";
import {
  useProjectLifecycle,
  type UseProjectLifecycleParams,
} from "@/hooks/useProjectLifecycle";
import type {
  BackgroundConfig,
  Dispatch,
  ExportHook,
  MutableRefObject,
  Project,
  ProjectComposition,
  ProjectsState,
  RefObject,
  SetStateAction,
  VideoSegment,
} from "@/hooks/appControllerTypes";
import type { PersistRef } from "@/hooks/appControllerTypes";

type LifecycleArg<K extends keyof UseProjectLifecycleParams> =
  UseProjectLifecycleParams[K];

export interface AppProjectLifecycleControllerArgs {
  abortEditorInteractions: LifecycleArg<"abortEditorInteractions">;
  applyLoadedBackgroundConfig: LifecycleArg<"applyLoadedBackgroundConfig">;
  armProjectInteractionShieldRelease: LifecycleArg<"armProjectInteractionShieldRelease">;
  backgroundConfig: BackgroundConfig;
  beginProjectInteractionShield: LifecycleArg<"beginProjectInteractionShield">;
  canvasRef: RefObject<HTMLCanvasElement | null>;
  clearClipMediaCaches: LifecycleArg<"clearClipMediaCaches">;
  clipExportSourcePathCacheRef: LifecycleArg<"clipExportSourcePathCacheRef">;
  clipExportWebcamPathCacheRef: LifecycleArg<"clipExportWebcamPathCacheRef">;
  clipLoadRequestSeqRef: LifecycleArg<"clipLoadRequestSeqRef">;
  composition: ProjectComposition | null;
  currentAudio: string | null;
  currentMicAudio: string | null;
  currentProjectData: Project | null;
  currentProjectDataRef: MutableRefObject<Project | null>;
  currentProjectIdRef: MutableRefObject<string | null>;
  currentProjectId: string | null;
  currentRawMicAudioPath: string;
  currentRawVideoPath: string;
  currentRawWebcamVideoPath: string;
  currentRecordingMode: LifecycleArg<"currentRecordingMode">;
  currentTime: number;
  currentVideo: string | null;
  currentWebcamVideo: string | null;
  duration: number;
  exportHook: ExportHook;
  flashRawSavedButton: LifecycleArg<"flashRawSavedButton">;
  generateThumbnail: LifecycleArg<"generateThumbnail">;
  getMonitors: LifecycleArg<"getMonitors">;
  handleProjectRawVideoPathChange: (path: string) => void;
  handleStopRecording: LifecycleArg<"handleStopRecording">;
  isPlaying: boolean;
  isProjectInteractionShieldVisible: boolean;
  isProjectTransitionRef: LifecycleArg<"isProjectTransitionRef">;
  isRecording: boolean;
  isSwitchingCompositionClipRef: LifecycleArg<"isSwitchingCompositionClipRef">;
  loadClipAssets: LifecycleArg<"loadClipAssets">;
  loadClipMediaIntoEditor: LifecycleArg<"loadClipMediaIntoEditor">;
  loadedClipId: string | null;
  monitors: LifecycleArg<"monitors">;
  mousePositions: LifecycleArg<"mousePositions">;
  onProjectLoadedRef: MutableRefObject<(project: Project) => void>;
  persistRef: PersistRef;
  previewContainerRef: MutableRefObject<HTMLDivElement | null>;
  projectInteractionBlockCleanupRef: LifecycleArg<"projectInteractionBlockCleanupRef">;
  projectInteractionShieldReleaseRef: LifecycleArg<"projectInteractionShieldReleaseRef">;
  projects: ProjectsState;
  projectsPreviewTargetSnapshotRef: LifecycleArg<"projectsPreviewTargetSnapshotRef">;
  rawAutoCopyEnabled: boolean;
  rawSaveDir: string;
  rawSetCurrentRawMicAudioPath: (path: string) => void;
  rawSetCurrentRawWebcamVideoPath: (path: string) => void;
  rawSetCurrentRecordingMode: LifecycleArg<"setCurrentRecordingMode">;
  restoreImageRef: LifecycleArg<"restoreImageRef">;
  seek: (time: number) => void;
  segment: VideoSegment | null;
  setComposition: LifecycleArg<"setComposition">;
  setCompositionSilently: LifecycleArg<"setCompositionSilently">;
  setCurrentAudio: Dispatch<SetStateAction<string | null>>;
  setCurrentMicAudio: Dispatch<SetStateAction<string | null>>;
  setCurrentProjectData: LifecycleArg<"setCurrentProjectData">;
  setCurrentTime: Dispatch<SetStateAction<number>>;
  setCurrentVideo: Dispatch<SetStateAction<string | null>>;
  setCurrentWebcamVideo: Dispatch<SetStateAction<string | null>>;
  setError: LifecycleArg<"setError">;
  setIsProjectInteractionShieldVisible: LifecycleArg<"setIsProjectInteractionShieldVisible">;
  setIsRawActionBusy: LifecycleArg<"setIsRawActionBusy">;
  setLastCaptureFps: LifecycleArg<"setLastCaptureFps">;
  setLastRawSavedPath: LifecycleArg<"setLastRawSavedPath">;
  setLoadedClipId: LifecycleArg<"setLoadedClipId">;
  setMousePositions: (positions: LifecycleArg<"mousePositions">) => void;
  setPreviewAudioResetKey: Dispatch<SetStateAction<number>>;
  setPreviewDuration: (duration: number) => void;
  setRawButtonSavedFlash: LifecycleArg<"setRawButtonSavedFlash">;
  setShowRawVideoDialog: LifecycleArg<"setShowRawVideoDialog">;
  setSpreadFromClipId: LifecycleArg<"setSpreadFromClipId">;
  setWebcamConfig: LifecycleArg<"setWebcamConfig">;
  spreadAnimationTimerRef: LifecycleArg<"spreadAnimationTimerRef">;
  startNewRecording: LifecycleArg<"startNewRecording">;
  videoControllerRef: MutableRefObject<
    import("@/lib/videoController").VideoController | undefined
  >;
  webcamConfig: LifecycleArg<"webcamConfig">;
}

export function useAppProjectLifecycleController(args: AppProjectLifecycleControllerArgs) {
  const {
    abortEditorInteractions,
    applyLoadedBackgroundConfig,
    armProjectInteractionShieldRelease,
    backgroundConfig,
    beginProjectInteractionShield,
    canvasRef,
    clearClipMediaCaches,
    clipExportSourcePathCacheRef,
    clipExportWebcamPathCacheRef,
    clipLoadRequestSeqRef,
    composition,
    currentAudio,
    currentMicAudio,
    currentProjectData,
    currentProjectDataRef,
    currentProjectId,
    currentProjectIdRef,
    currentRawMicAudioPath,
    currentRawVideoPath,
    currentRawWebcamVideoPath,
    currentRecordingMode,
    currentTime,
    currentVideo,
    currentWebcamVideo,
    duration,
    exportHook,
    flashRawSavedButton,
    generateThumbnail,
    getMonitors,
    handleProjectRawVideoPathChange,
    handleStopRecording,
    isPlaying,
    isProjectInteractionShieldVisible,
    isProjectTransitionRef,
    isRecording,
    isSwitchingCompositionClipRef,
    loadClipAssets,
    loadClipMediaIntoEditor,
    loadedClipId,
    monitors,
    mousePositions,
    onProjectLoadedRef,
    previewContainerRef,
    projectInteractionBlockCleanupRef,
    projectInteractionShieldReleaseRef,
    projects,
    projectsPreviewTargetSnapshotRef,
    rawAutoCopyEnabled,
    rawSaveDir,
    rawSetCurrentRawMicAudioPath,
    rawSetCurrentRawWebcamVideoPath,
    rawSetCurrentRecordingMode,
    restoreImageRef,
    seek,
    segment,
    setComposition,
    setCompositionSilently,
    setCurrentAudio,
    setCurrentMicAudio,
    setCurrentProjectData,
    setCurrentVideo,
    setCurrentWebcamVideo,
    setError,
    setIsProjectInteractionShieldVisible,
    setIsRawActionBusy,
    setLastCaptureFps,
    setLastRawSavedPath,
    setLoadedClipId,
    setPreviewAudioResetKey,
    setRawButtonSavedFlash,
    setShowRawVideoDialog,
    setSpreadFromClipId,
    setWebcamConfig,
    spreadAnimationTimerRef,
    startNewRecording,
    videoControllerRef,
    webcamConfig,
  } = args;

  const lifecycle = useProjectLifecycle({
    persistRef: args.persistRef,
    isProjectTransitionRef,
    isSwitchingCompositionClipRef,
    canvasRef,
    previewContainerRef,
    restoreImageRef,
    projectsPreviewTargetSnapshotRef,
    currentProjectId,
    currentProjectIdRef,
    projects: {
      projects: projects.projects,
      loadProjects: projects.loadProjects,
      refreshProjectEntry: projects.refreshProjectEntry,
      setCurrentProjectId: projects.setCurrentProjectId,
    },
    currentVideo,
    currentAudio,
    currentMicAudio,
    currentWebcamVideo,
    loadedClipId,
    currentProjectData,
    currentProjectDataRef,
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
    setCompositionSilently,
    setCurrentProjectData,
    monitors,
    getMonitors,
    isRecording,
    startNewRecording,
    setError,
    setCurrentRecordingMode: rawSetCurrentRecordingMode,
    setCurrentRawVideoPath: handleProjectRawVideoPathChange,
    setCurrentRawMicAudioPath: rawSetCurrentRawMicAudioPath,
    setCurrentRawWebcamVideoPath: rawSetCurrentRawWebcamVideoPath,
    setLastRawSavedPath,
    setRawButtonSavedFlash,
    projectsDialog: {
      showProjectsDialog: projects.showProjectsDialog,
      setShowProjectsDialog: projects.setShowProjectsDialog,
      handleLoadProject: projects.handleLoadProject,
    },
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
    setShowExportSuccessDialog: exportHook.setShowExportSuccessDialog,
    setIsRawActionBusy,
  });

  onProjectLoadedRef.current = lifecycle.onProjectLoaded;

  useMediaPipelineRecovery({
    currentProjectDataRef,
    currentRawMicAudioPath,
    currentRawVideoPath,
    currentRawWebcamVideoPath,
    currentTime,
    isPlaying,
    projectId: currentProjectId,
    seek,
    segmentDeviceAudioAvailable: segment?.deviceAudioAvailable,
    setCurrentAudio,
    setCurrentMicAudio,
    setCurrentVideo,
    setCurrentWebcamVideo,
    setPreviewAudioResetKey,
    videoControllerRef,
  });

  return lifecycle;
}
