import { useAppEffects } from "@/hooks/useAppEffects";
import { useCloseProject } from "@/hooks/useCloseProject";
import { useEditorInteractions } from "@/hooks/useEditorInteractions";
import { useCallback } from "react";
import { projectManager } from "@/lib/projectManager";
import { useSettings } from "@/hooks/useSettings";
import {
  clearProjectsAfterClosingActive,
  deleteProjectAfterClosingActive,
} from "@/lib/projectLifecycleActions";
import type { ActivePanel } from "@/components/sidepanel/index";
import type {
  BackgroundConfig,
  Dispatch,
  EditorHistory,
  ExportHook,
  MousePosition,
  MutableRefObject,
  Project,
  ProjectComposition,
  ProjectsState,
  RecordingMode,
  RefObject,
  SegmentSetter,
  SetStateAction,
  VideoSegment,
  WebcamConfig,
} from "@/hooks/appControllerTypes";
import type { PersistRef } from "@/hooks/appControllerTypes";

export interface AppLateEffectsArgs {
  backgroundConfig: BackgroundConfig;
  beginBatch: () => void;
  canRedo: boolean;
  canUndo: boolean;
  canvasRef: RefObject<HTMLCanvasElement | null>;
  commitBatch: () => void;
  composition: ProjectComposition | null;
  currentAudio: string | null;
  currentMicAudio: string | null;
  currentProjectId: string | null;
  currentProjectDataRef: MutableRefObject<Project | null>;
  currentProjectIdRef: MutableRefObject<string | null>;
  currentRawMicAudioPath: string;
  currentRawVideoPath: string;
  currentRawWebcamVideoPath: string;
  currentRecordingMode: RecordingMode;
  currentTime: number;
  currentVideo: string | null;
  currentWebcamVideo: string | null;
  duration: number;
  editingKeystrokeSegmentId: string | null;
  editingKeyframeId: number | null;
  editingPointerId: string | null;
  editingSubtitleId: string | null;
  editingTextId: string | null;
  editorHistory: EditorHistory;
  exportHook: ExportHook;
  getKeystrokeTimelineDuration: (s: VideoSegment) => number;
  handleDeleteKeystrokeSegment: () => void;
  handleDeletePointerSegment: () => void;
  handleDeleteSubtitle: () => void;
  handleDeleteText: () => void;
  handleOverlayDragMove: (
    moves: Array<{ kind: "text" | "subtitle"; id: string; x: number; y: number }>,
  ) => void;
  handleStartRecording: () => void;
  handleTogglePlayPause: () => void;
  historyProjectResetRef: MutableRefObject<string | null>;
  isCropping: boolean;
  isDraggingKeystrokeOverlayRef: MutableRefObject<boolean>;
  isRecording: boolean;
  beginProjectInteractionShield: () => void;
  endProjectInteractionShield: () => void;
  isResizingKeystrokeOverlayRef: MutableRefObject<boolean>;
  mousePositions: MousePosition[];
  onStopRecording: () => void;
  persistRef: PersistRef;
  projects: ProjectsState;
  rawSetComposition: Dispatch<SetStateAction<ProjectComposition | null>>;
  rawSetSegment: Dispatch<SetStateAction<VideoSegment | null>>;
  redo: () => void;
  seek: (time: number) => void;
  segment: VideoSegment | null;
  segmentRef: MutableRefObject<VideoSegment | null>;
  selectedSubtitleIdsRef: MutableRefObject<string[]>;
  selectedTextIdsRef: MutableRefObject<string[]>;
  setActivePanel: (panel: ActivePanel) => void;
  setCurrentAudio: Dispatch<SetStateAction<string | null>>;
  setCurrentMicAudio: Dispatch<SetStateAction<string | null>>;
  setCurrentProjectData: Dispatch<SetStateAction<Project | null>>;
  setCurrentTime: Dispatch<SetStateAction<number>>;
  setCurrentVideo: Dispatch<SetStateAction<string | null>>;
  setCurrentWebcamVideo: Dispatch<SetStateAction<string | null>>;
  setEditingKeyframeId: (id: number | null) => void;
  setEditingSubtitleId: (id: string | null) => void;
  setEditingTextId: (id: string | null) => void;
  setIsKeystrokeOverlaySelected: (selected: boolean) => void;
  setIsKeystrokeResizeDragging: (dragging: boolean) => void;
  setIsKeystrokeResizeHandleHover: (hover: boolean) => void;
  setIsPreviewDragging: (dragging: boolean) => void;
  setLoadedClipId: (id: string | null) => void;
  setMousePositions: Dispatch<SetStateAction<MousePosition[]>>;
  setPreviewDuration: Dispatch<SetStateAction<number>>;
  setSeekIndicatorDir: (dir: "left" | "right") => void;
  setSeekIndicatorKey: (key: number) => void;
  setSegment: SegmentSetter;
  setThumbnails: Dispatch<SetStateAction<string[]>>;
  setError: (message: string) => void;
  showHotkeyDialog: boolean;
  showRawVideoDialog: boolean;
  tempCanvasRef: RefObject<HTMLCanvasElement | null>;
  undo: () => void;
  videoRef: RefObject<HTMLVideoElement | null>;
  webcamConfig: WebcamConfig;
}

export function useAppLateEffects(args: AppLateEffectsArgs) {
  const { t } = useSettings();
  const {
    backgroundConfig,
    beginBatch,
    canRedo,
    canUndo,
    canvasRef,
    commitBatch,
    composition,
    currentAudio,
    currentMicAudio,
    currentProjectId,
    currentProjectDataRef,
    currentProjectIdRef,
    currentRawMicAudioPath,
    currentRawVideoPath,
    currentRawWebcamVideoPath,
    currentRecordingMode,
    currentTime,
    currentVideo,
    currentWebcamVideo,
    duration,
    editingKeystrokeSegmentId,
    editingKeyframeId,
    editingPointerId,
    editingSubtitleId,
    editingTextId,
    editorHistory,
    exportHook,
    getKeystrokeTimelineDuration,
    handleDeleteKeystrokeSegment,
    handleDeletePointerSegment,
    handleDeleteSubtitle,
    handleDeleteText,
    handleStartRecording,
    handleTogglePlayPause,
    historyProjectResetRef,
    isCropping,
    isDraggingKeystrokeOverlayRef,
    isRecording,
    beginProjectInteractionShield,
    endProjectInteractionShield,
    isResizingKeystrokeOverlayRef,
    mousePositions,
    onStopRecording,
    persistRef,
    projects,
    rawSetComposition,
    rawSetSegment,
    redo,
    seek,
    segment,
    segmentRef,
    selectedSubtitleIdsRef,
    selectedTextIdsRef,
    setActivePanel,
    setCurrentAudio,
    setCurrentMicAudio,
    setCurrentProjectData,
    setCurrentTime,
    setCurrentVideo,
    setCurrentWebcamVideo,
    setEditingKeyframeId,
    setEditingSubtitleId,
    setEditingTextId,
    setIsKeystrokeOverlaySelected,
    setIsKeystrokeResizeDragging,
    setIsKeystrokeResizeHandleHover,
    setIsPreviewDragging,
    setLoadedClipId,
    setMousePositions,
    setPreviewDuration,
    setSeekIndicatorDir,
    setSeekIndicatorKey,
    setSegment,
    setThumbnails,
    setError,
    showHotkeyDialog,
    showRawVideoDialog,
    tempCanvasRef,
    undo,
    videoRef,
    webcamConfig,
  } = args;

  useAppEffects({
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
    isProcessing: exportHook.isProcessing,
  });

  const handleCloseProject = useCloseProject({
    backgroundConfig,
    currentAudio,
    currentMicAudio,
    currentRawMicAudioPath,
    currentRawVideoPath,
    currentRawWebcamVideoPath,
    currentRecordingMode,
    currentProjectDataRef,
    currentProjectId,
    currentProjectIdRef,
    currentVideo,
    currentWebcamVideo,
    editorHistory,
    historyProjectResetRef,
    isProcessing: exportHook.isProcessing,
    isRecording,
    beginProjectInteractionShield,
    endProjectInteractionShield,
    persistRef,
    projects,
    rawSetComposition,
    rawSetSegment,
    setCurrentAudio,
    setCurrentMicAudio,
    setCurrentProjectData,
    setCurrentTime,
    setCurrentVideo,
    setCurrentWebcamVideo,
    setLoadedClipId,
    setMousePositions,
    setPreviewDuration,
    setThumbnails,
    setError,
    webcamConfig,
  });

  const handleRenameProject = useCallback(async (id: string, name: string) => {
    try {
      await projectManager.renameProject(id, name);
      await projects.loadProjects();
    } catch (error) {
      console.error("Could not rename project", error);
      setError(t.projectRenameFailed);
    }
  }, [projects, setError, t.projectRenameFailed]);

  const handleDeleteProject = useCallback(async (id: string) => {
    try {
      await deleteProjectAfterClosingActive(id, currentProjectId, {
        closeProject: handleCloseProject,
        deleteProject: (projectId) => projectManager.deleteProject(projectId),
        reloadProjects: projects.loadProjects,
      });
    } catch (error) {
      console.error("Could not delete project", error);
      setError(t.projectDeleteFailed);
    }
  }, [currentProjectId, handleCloseProject, projects, setError, t.projectDeleteFailed]);

  const handleClearProjects = useCallback(async () => {
    try {
      await clearProjectsAfterClosingActive(
        projects.projects.map((project) => project.id),
        currentProjectId,
        {
          closeProject: handleCloseProject,
          deleteProject: (projectId) => projectManager.deleteProject(projectId),
          reloadProjects: projects.loadProjects,
        },
      );
    } catch (error) {
      console.error("Could not clear projects", error);
      setError(t.projectClearFailed);
    }
  }, [currentProjectId, handleCloseProject, projects, setError, t.projectClearFailed]);

  useEditorInteractions({
    segment,
    setSegment,
    currentTime,
    duration,
    backgroundConfig,
    canvasRef,
    videoRef,
    seek,
    isCropping,
    isModalOpen: showRawVideoDialog || exportHook.showExportSuccessDialog,
    editingKeyframeId,
    editingTextId,
    editingSubtitleId,
    editingKeystrokeSegmentId,
    editingPointerId,
    setEditingKeyframeId,
    handleDeleteText,
    handleDeleteSubtitle,
    handleDeleteKeystrokeSegment,
    handleDeletePointerSegment,
    canUndo,
    canRedo,
    undo,
    redo,
    setSeekIndicatorKey,
    setSeekIndicatorDir,
    handleTogglePlayPause,
    mousePositions,
    currentMicAudio,
    currentWebcamVideo,
    tempCanvasRef,
    segmentRef,
    isDraggingKeystrokeOverlayRef,
    isResizingKeystrokeOverlayRef,
    getKeystrokeTimelineDuration,
    setIsPreviewDragging,
    setIsKeystrokeResizeDragging,
    setIsKeystrokeResizeHandleHover,
    setIsKeystrokeOverlaySelected,
    setEditingTextId,
    setEditingSubtitleId,
    setActivePanel,
    handleOverlayDragMove: args.handleOverlayDragMove,
    selectedTextIdsRef,
    selectedSubtitleIdsRef,
    beginBatch,
    commitBatch,
  });

  return {
    handleClearProjects,
    handleCloseProject,
    handleDeleteProject,
    handleRenameProject,
  };
}
