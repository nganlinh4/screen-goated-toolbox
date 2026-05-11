import { startTransition, useCallback, useEffect, useRef, useState } from "react";
import "./App.css";
import {
  BackgroundConfig, Project, ProjectComposition,
  VideoSegment, RecordingMode, WebcamConfig, ImportedAudioSegment, NarrationSegment,
} from "@/types/video";

import { useEditorHistory, type EditorHistorySnapshot } from "@/hooks/useEditorHistory";
import { useHotkeys, useMonitors, useWindows } from "@/hooks/useAppHooks";
import { useProjects, useExport } from "@/hooks/useVideoState";
import { useMediaEngine } from "@/hooks/useMediaEngine";
import { getInitialBackgroundConfig } from "@/lib/appUtils";
import { logToHost } from "@/lib/ipc";
import { useDebugEffects } from "@/hooks/useDebugEffects";
import { useAppEffects } from "@/hooks/useAppEffects";
import { useBackgroundManager } from "@/hooks/useBackgroundManager";
import { useEditorTools } from "@/hooks/useEditorTools";
import { useEditorInteractions } from "@/hooks/useEditorInteractions";

import { Header } from "@/components/Header";
import { SequencePillChain } from "@/components/SequencePillChain";
import { type ActivePanel } from "@/components/sidepanel/index";
import { type ProjectsPreviewTargetSnapshot } from "@/components/ProjectsView";
import { SettingsContext, useSettingsProvider } from "@/hooks/useSettings";
import { ResizeBorders } from "@/components/layout/ResizeBorders";
import { useRawVideoHandler } from "@/hooks/useRawVideoHandler";
import { useProjectLifecycle } from "@/hooks/useProjectLifecycle";
import { useTimelineAdaptiveThumbnails } from "@/hooks/useTimelineAdaptiveThumbnails";
import { useCompositionPipeline } from "@/hooks/useCompositionPipeline";
import { type PersistOptions } from "@/hooks/useSequenceComposition";
import { EditorOverlays } from "@/components/EditorOverlays";
import { DragDropOverlay } from "@/components/DragDropOverlay";
import { useVideoImport } from "@/hooks/useVideoImport";
import { useImportedAudioImport } from "@/hooks/useImportedAudioImport";
import { useAudioDownload } from "@/hooks/useAudioDownload";
import { useSubtitleImport } from "@/hooks/useSubtitleSrtImport";
import type { SubtitleFileFormat } from "@/lib/subtitleSrt";
import { useSubtitleGeneration } from "@/hooks/useSubtitleGeneration";
import { EditorMain } from "@/components/EditorMain";
import { cloneBackgroundConfig } from "@/lib/backgroundConfig";
import { projectManager } from "@/lib/projectManager";
import { createAudioPlaceholderVideo, getMediaServerUrl } from "@/lib/mediaServer";
import {
  getTimelineContentEnd,
  resizeCompositionRootDuration,
  resizeSegmentDuration,
} from "@/lib/timelineDuration";
import { cloneWebcamConfig, DEFAULT_WEBCAM_CONFIG } from "@/lib/webcam";
import {
  deleteSubtitleIdsAcrossTracks,
  updateSubtitleStylesAcrossTracks,
} from "@/lib/subtitleTrackMutations";
import { installFrontendPerfDiagnostics } from "@/lib/frontendPerfDiagnostics";
import { invoke } from "@/lib/ipc";
import {
  applyLiveNarrationSegments,
  clearLiveNarrationSegments,
} from "@/lib/liveNarrationStreamStore";
import { installScreenRecordAppTestHarness } from "@/testHarness/appHarness";

type PendingVideoDropAction = {
  path?: string;
  action?: string;
};

type PendingSubtitleDropAction = {
  path?: string;
};

type ReadSubtitleFilePathResult = {
  fileName?: string;
  content?: string;
  format?: SubtitleFileFormat;
};

function preserveSilentAudioLanes(
  nextComposition: ProjectComposition | null,
  previousComposition: ProjectComposition | null | undefined,
  projectComposition: ProjectComposition | null | undefined,
) {
  if (!nextComposition) return nextComposition;
  const fallbackComposition = projectComposition ?? previousComposition;
  if (!fallbackComposition) return nextComposition;
  return {
    ...nextComposition,
    audioSegments:
      (nextComposition.audioSegments?.length ?? 0) === 0 &&
      (fallbackComposition.audioSegments?.length ?? 0) > 0
        ? fallbackComposition.audioSegments
        : nextComposition.audioSegments,
    narrationSegments:
      (nextComposition.narrationSegments?.length ?? 0) === 0 &&
      (fallbackComposition.narrationSegments?.length ?? 0) > 0
        ? fallbackComposition.narrationSegments
        : nextComposition.narrationSegments,
    audioTrackVolumePoints:
      (nextComposition.audioTrackVolumePoints?.length ?? 0) === 0 &&
      (fallbackComposition.audioTrackVolumePoints?.length ?? 0) > 0
        ? fallbackComposition.audioTrackVolumePoints
        : nextComposition.audioTrackVolumePoints,
    narrationTrackVolumePoints:
      (nextComposition.narrationTrackVolumePoints?.length ?? 0) === 0 &&
      (fallbackComposition.narrationTrackVolumePoints?.length ?? 0) > 0
        ? fallbackComposition.narrationTrackVolumePoints
        : nextComposition.narrationTrackVolumePoints,
  };
}

function App() {
  useEffect(() => {
    installFrontendPerfDiagnostics();
  }, []);

  const settings = useSettingsProvider();
  const [segment, rawSetSegment] = useState<VideoSegment | null>(null);
  const [activePanel, setActivePanel] = useState<ActivePanel>("background");
  const [isCropping, setIsCropping] = useState(false);
  const [backgroundConfig, setBackgroundConfigState] =
    useState<BackgroundConfig>(() => cloneBackgroundConfig(getInitialBackgroundConfig()));
  const [webcamConfig, rawSetWebcamConfig] = useState<WebcamConfig>(() => cloneWebcamConfig(DEFAULT_WEBCAM_CONFIG));
  const [currentRecordingMode, rawSetCurrentRecordingMode] = useState<RecordingMode>("withoutCursor");
  const [currentProjectData, setCurrentProjectData] = useState<Project | null>(null);
  const [composition, rawSetComposition] = useState<ProjectComposition | null>(null);
  const {
    currentRawVideoPath,
    setCurrentRawVideoPath: rawSetCurrentRawVideoPath,
    lastRawSavedPath,
    setLastRawSavedPath,
    showRawVideoDialog,
    setShowRawVideoDialog,
    rawAutoCopyEnabled,
    rawSaveDir,
    isRawActionBusy,
    setIsRawActionBusy,
    rawButtonSavedFlash,
    setRawButtonSavedFlash,
    flashRawSavedButton,
    handleProjectRawVideoPathChange,
    handleOpenRawVideoDialog,
    handleToggleRawAutoCopy,
  } = useRawVideoHandler();
  const [currentRawMicAudioPath, rawSetCurrentRawMicAudioPath] = useState("");
  const [currentRawWebcamVideoPath, rawSetCurrentRawWebcamVideoPath] = useState("");
  const [timelineCanvasWidthPx, setTimelineCanvasWidthPx] = useState(0);
  const [previewAudioResetKey, setPreviewAudioResetKey] = useState(0);
  const isTimelineOnlyProject = Boolean(
    segment?.mediaMode === "timelineOnly" ||
    composition?.timelineOnly,
  );
  const isPlaceholderBackedProject = Boolean(
    composition?.placeholderVideoForAudio ||
    composition?.placeholderVideoForSubtitles ||
    composition?.timelineOnly ||
    segment?.mediaMode === "timelineOnly",
  );

  const timelineRef = useRef<HTMLDivElement>(null);
  const previewContainerRef = useRef<HTMLDivElement>(null);
  const restoreImageRef = useRef<string | null>(null);
  const projectsPreviewTargetSnapshotRef = useRef<ProjectsPreviewTargetSnapshot | null>(null);
  const segmentRef = useRef<VideoSegment | null>(null);
  const selectedTextIdsRef = useRef<string[]>([]);
  const selectedSubtitleIdsRef = useRef<string[]>([]);
  const isDraggingKeystrokeOverlayRef = useRef(false);
  const isResizingKeystrokeOverlayRef = useRef(false);
  const [isCanvasResizeDragging, setIsCanvasResizeDragging] = useState(false);
  // Stable ref for onProjectLoaded — breaks circular dep between useClipMediaCache and useProjects
  const onProjectLoadedRef = useRef<(project: Project) => void>(null!);
  const currentProjectIdRef = useRef<string | null>(null);
  const currentProjectDataRef = useRef<Project | null>(null);
  const narrationApplyPerfSeqRef = useRef(0);
  const hasDeferredNarrationEditorFlushRef = useRef(false);
  // Stable ref for persist callback — avoids cascading useEffect re-triggers
  const persistRef = useRef<((opts?: PersistOptions) => Promise<void>) | null>(null);
  const compositionPersistChainRef = useRef<Promise<void>>(Promise.resolve());
  // Early ref so setBackgroundConfig can guard against mid-transition mutations
  const isProjectTransitionRef = useRef(false);
  useEffect(() => {
    currentProjectDataRef.current = currentProjectData;
  }, [currentProjectData]);

  const {
    backgroundMutationMetaRef,
    setBackgroundConfig: rawSetBackgroundConfig,
    applyLoadedBackgroundConfig,
    recentUploads,
    isBackgroundUploadProcessing,
    handleBackgroundUpload,
    handleRemoveRecentUpload,
  } = useBackgroundManager({
    backgroundConfig,
    setBackgroundConfigState,
    isProjectTransitionRef,
  });

  const {
    hotkeys,
    showHotkeyDialog,
    handleRemoveHotkey,
    openHotkeyDialog,
    closeHotkeyDialog,
  } = useHotkeys();
  const { monitors, getMonitors } = useMonitors();
  const { windows, showWindowSelect, setShowWindowSelect, getWindows } = useWindows();

  const {
    currentTime,
    setCurrentTime,
    duration,
    setDuration: setPreviewDuration,
    isPlaying,
    isBuffering,
    isVideoReady,
    thumbnails,
    setThumbnails,
    currentVideo,
    setCurrentVideo,
    currentAudio,
    setCurrentAudio,
    currentMicAudio,
    setCurrentMicAudio,
    currentWebcamVideo,
    setCurrentWebcamVideo,
    videoRef,
    webcamVideoRef,
    audioRef,
    micAudioRef,
    canvasRef,
    tempCanvasRef,
    videoControllerRef,
    renderFrame,
    togglePlayback,
    seek,
    flushSeek,
    generateThumbnail,
    generateThumbnailsForSource,
    invalidateThumbnails,
    isRecording,
    recordingDuration,
    isLoadingVideo,
    loadingProgress,
    mousePositions,
    setMousePositions,
    audioFilePath,
    micAudioFilePath,
    webcamVideoFilePath,
    videoFilePath,
    videoFilePathOwnerUrl,
    error,
    setError,
    startNewRecording,
    handleStopRecording,
  } = useMediaEngine({
    segment,
    backgroundConfig,
    webcamConfig,
    isCropping,
    isCanvasResizeDragging,
    isTimelineOnly: isTimelineOnlyProject,
    setSegment: rawSetSegment,
  });

  const applyHistorySnapshot = useCallback((snapshot: EditorHistorySnapshot) => {
    rawSetSegment(snapshot.segment);
    rawSetComposition(snapshot.composition);
    setBackgroundConfigState(cloneBackgroundConfig(snapshot.backgroundConfig));
    rawSetWebcamConfig(cloneWebcamConfig(snapshot.webcamConfig));
    setPreviewDuration(snapshot.duration);
    rawSetCurrentRecordingMode(snapshot.currentRecordingMode);
    rawSetCurrentRawVideoPath(snapshot.currentRawVideoPath);
    setLastRawSavedPath("");
    rawSetCurrentRawMicAudioPath(snapshot.currentRawMicAudioPath);
    rawSetCurrentRawWebcamVideoPath(snapshot.currentRawWebcamVideoPath);
    const applyProjectSnapshot = (project: Project): Project => ({
          ...project,
          duration: snapshot.duration,
          segment: snapshot.segment ?? project.segment,
          backgroundConfig: cloneBackgroundConfig(snapshot.backgroundConfig),
          webcamConfig: cloneWebcamConfig(snapshot.webcamConfig),
          composition: snapshot.composition ?? undefined,
          rawVideoPath: snapshot.currentRawVideoPath || undefined,
          rawMicAudioPath: snapshot.currentRawMicAudioPath || undefined,
          rawWebcamVideoPath: snapshot.currentRawWebcamVideoPath || undefined,
        });
    currentProjectDataRef.current = currentProjectDataRef.current
      ? applyProjectSnapshot(currentProjectDataRef.current)
      : currentProjectDataRef.current;
    setCurrentProjectData((prev) => prev ? applyProjectSnapshot(prev) : prev);
  }, [
    rawSetCurrentRawVideoPath,
    setLastRawSavedPath,
    setPreviewDuration,
  ]);

  const editorHistory = useEditorHistory({
    initialSnapshot: {
      segment,
      composition,
      backgroundConfig,
      webcamConfig,
      duration,
      currentRecordingMode,
      currentRawVideoPath,
      currentRawMicAudioPath,
      currentRawWebcamVideoPath,
    },
    applySnapshot: applyHistorySnapshot,
  });
  const {
    undo,
    redo,
    canUndo,
    canRedo,
    isBatching,
    beginBatch,
    commitBatch,
  } = editorHistory;

  const setSegment = useCallback((
    value: VideoSegment | null | ((prev: VideoSegment | null) => VideoSegment | null),
  ) => {
    editorHistory.setSegment(value);
    rawSetSegment(value);
  }, [editorHistory]);

  const setComposition = useCallback((
    value: ProjectComposition | null | ((prev: ProjectComposition | null) => ProjectComposition | null),
  ) => {
    editorHistory.setComposition(value);
    rawSetComposition(value);
  }, [editorHistory]);

  const setCompositionSilently = useCallback((
    value: ProjectComposition | null | ((prev: ProjectComposition | null) => ProjectComposition | null),
  ) => {
    rawSetComposition((prev) => {
      const next = typeof value === "function"
        ? (value as (current: ProjectComposition | null) => ProjectComposition | null)(prev)
        : value;
      return preserveSilentAudioLanes(
        next,
        prev,
        currentProjectDataRef.current?.composition ?? null,
      );
    });
  }, []);

  const setEditorPreviewDuration = useCallback((value: number) => {
    editorHistory.setDuration(value);
    setPreviewDuration(value);
  }, [editorHistory, setPreviewDuration]);

  const handleEditorRawVideoPathChange = useCallback((value: string) => {
    editorHistory.setCurrentRawVideoPath(value);
    handleProjectRawVideoPathChange(value);
  }, [editorHistory, handleProjectRawVideoPathChange]);

  const setBackgroundConfig = useCallback((
    value: BackgroundConfig | ((prev: BackgroundConfig) => BackgroundConfig),
  ) => {
    editorHistory.setBackgroundConfig(value);
    rawSetBackgroundConfig(value);
  }, [editorHistory, rawSetBackgroundConfig]);

  const setWebcamConfig = useCallback((
    value: WebcamConfig | ((prev: WebcamConfig) => WebcamConfig),
  ) => {
    editorHistory.setWebcamConfig(value);
    rawSetWebcamConfig(value);
  }, [editorHistory]);

  useEffect(() => {
    editorHistory.replaceSnapshot({
      segment,
      composition,
      backgroundConfig,
      webcamConfig,
      duration,
      currentRecordingMode,
      currentRawVideoPath,
      currentRawMicAudioPath,
      currentRawWebcamVideoPath,
    });
  }, [
    editorHistory,
    segment,
    composition,
    backgroundConfig,
    webcamConfig,
    duration,
    currentRecordingMode,
    currentRawVideoPath,
    currentRawMicAudioPath,
    currentRawWebcamVideoPath,
  ]);

  const projects = useProjects({
    videoControllerRef,
    setCurrentVideo,
    setCurrentAudio,
    setCurrentMicAudio,
    setCurrentWebcamVideo,
    setSegment: rawSetSegment,
    setBackgroundConfig: rawSetBackgroundConfig,
    setWebcamConfig: rawSetWebcamConfig,
    applyLoadedBackgroundConfig,
    setMousePositions,
    setThumbnails,
    setCurrentRecordingMode: rawSetCurrentRecordingMode,
    setCurrentRawVideoPath: handleProjectRawVideoPathChange,
    onProjectLoaded: (project) => onProjectLoadedRef.current(project),
    currentVideo,
    currentAudio,
    currentMicAudio,
    currentWebcamVideo,
  });
  useEffect(() => {
    currentProjectIdRef.current = projects.currentProjectId;
  }, [projects.currentProjectId]);
  useEffect(() => {
    return installScreenRecordAppTestHarness({
      loadProject: (project) => {
        editorHistory.withoutHistory(() => {
          currentProjectIdRef.current = project.id;
          currentProjectDataRef.current = project;
          setCurrentProjectData(project);
          rawSetSegment(project.segment);
          rawSetComposition(project.composition ?? null);
          setBackgroundConfigState(cloneBackgroundConfig(project.backgroundConfig));
          rawSetWebcamConfig(cloneWebcamConfig(project.webcamConfig ?? DEFAULT_WEBCAM_CONFIG));
          setPreviewDuration(project.duration ?? project.segment.trimEnd);
          setCurrentTime(0);
          handleProjectRawVideoPathChange(project.rawVideoPath ?? "");
          rawSetCurrentRawMicAudioPath(project.rawMicAudioPath ?? "");
          rawSetCurrentRawWebcamVideoPath(project.rawWebcamVideoPath ?? "");
          setCurrentVideo(null);
          setCurrentAudio(null);
          setCurrentMicAudio(null);
          setCurrentWebcamVideo(null);
          setThumbnails([]);
          setMousePositions(project.mousePositions ?? []);
        });
        projects.setCurrentProjectId(project.id);
        editorHistory.resetHistory({
          segment: project.segment,
          composition: project.composition ?? null,
          backgroundConfig: project.backgroundConfig,
          webcamConfig: project.webcamConfig ?? DEFAULT_WEBCAM_CONFIG,
          duration: project.duration ?? project.segment.trimEnd,
          currentRecordingMode,
          currentRawVideoPath: project.rawVideoPath ?? "",
          currentRawMicAudioPath: project.rawMicAudioPath ?? "",
          currentRawWebcamVideoPath: project.rawWebcamVideoPath ?? "",
        });
      },
      getProjectId: () => currentProjectIdRef.current,
      getDuration: () => duration,
      getSegment: () => currentProjectDataRef.current?.segment ?? segmentRef.current ?? segment,
      getComposition: () => currentProjectDataRef.current?.composition ?? composition,
      setCurrentTime,
    });
  }, [
    composition,
    currentRecordingMode,
    currentRawMicAudioPath,
    currentRawVideoPath,
    currentRawWebcamVideoPath,
    duration,
    editorHistory,
    handleProjectRawVideoPathChange,
    projects,
    segment,
    setCurrentAudio,
    setCurrentMicAudio,
    setCurrentVideo,
    setCurrentWebcamVideo,
    setMousePositions,
    setPreviewDuration,
    setThumbnails,
    setCurrentTime,
  ]);
  const historyProjectResetRef = useRef<string | null>(null);
  useEffect(() => {
    const projectId = currentProjectData?.id ?? null;
    if (!projectId || historyProjectResetRef.current === projectId) return;
    historyProjectResetRef.current = projectId;
    editorHistory.resetHistory(editorHistory.getSnapshot());
  }, [currentProjectData?.id, editorHistory]);

  const {
    loadedClipId,
    setLoadedClipId,
    isSwitchingCompositionClipRef,
    clipExportSourcePathCacheRef,
    clipExportWebcamPathCacheRef,
    clipLoadRequestSeqRef,
    previousPreloadVideoRef,
    previousPreloadAudioRef,
    nextPreloadVideoRef,
    nextPreloadAudioRef,
    loadClipAssets,
    clearClipMediaCaches,
    loadClipMediaIntoEditor,
    resolveClipExportSourcePath,
    resolveClipExportMicAudioPath,
    resolveClipExportWebcamPath,
    isProjectInteractionShieldVisible,
    setIsProjectInteractionShieldVisible,
    projectInteractionShieldReleaseRef,
    projectInteractionBlockCleanupRef,
    beginProjectInteractionShield,
    abortEditorInteractions,
    armProjectInteractionShieldRelease,
    projectPickerMode,
    setProjectPickerMode,
    setSpreadFromClipId,
    spreadFromClipId,
    spreadAnimationTimerRef,
    hasSequenceChain,
    selectedClipId,
    activeClipId,
    handleTogglePlayPause: handleVideoTogglePlayPause,
    handleOpenInsertProjectPicker,
    handlePickProjectForSequence,
    handleSelectSequenceClip,
    handleRemoveSequenceClip,
    handleSequenceModeChange,
  } = useCompositionPipeline({
    composition,
    setComposition,
    setCompositionSilently,
    currentProjectData,
    setCurrentProjectData,
    segment,
    backgroundConfig,
    mousePositions,
    duration,
    currentRawVideoPath,
    currentRecordingMode,
    currentProjectId: projects.currentProjectId,
    currentVideo,
    currentAudio,
    currentMicAudio,
    currentWebcamVideo,
    setCurrentVideo,
    setCurrentAudio,
    setCurrentMicAudio,
    setCurrentWebcamVideo,
    setPreviewDuration,
    setThumbnails,
    generateThumbnailsForSource,
    invalidateThumbnails,
    setSegment,
    videoControllerRef,
    webcamVideoRef,
    canvasRef,
    tempCanvasRef,
    previewContainerRef,
    isProjectTransitionRef,
    persistRef,
    applyLoadedBackgroundConfig,
    setWebcamConfig: rawSetWebcamConfig,
    setMousePositions,
    setCurrentRecordingMode: rawSetCurrentRecordingMode,
    handleProjectRawVideoPathChange,
    setCurrentRawMicAudioPath: rawSetCurrentRawMicAudioPath,
    setCurrentRawWebcamVideoPath: rawSetCurrentRawWebcamVideoPath,
    showProjectsDialog: projects.showProjectsDialog,
    setShowProjectsDialog: projects.setShowProjectsDialog,
    seek,
    isPlaying,
    currentTime,
    togglePlayback,
  });

  useTimelineAdaptiveThumbnails({
    timelineCanvasWidthPx,
    segment,
    currentVideo,
    currentRawVideoPath,
    thumbnailsLength: thumbnails.length,
    isPlaying,
    generateThumbnailsForSource,
  });

  useDebugEffects({
    backgroundConfig,
    isProjectTransitionRef,
    isSwitchingCompositionClipRef,
    isCropping,
    currentProjectId: projects.currentProjectId,
    showProjectsDialog: projects.showProjectsDialog,
    backgroundMutationMetaRef,
    currentTime,
    currentVideo,
    isRecording,
    isLoadingVideo,
    isPlaying,
    isVideoReady,
    hasSequenceChain,
    loadedClipId,
    selectedClipId,
  });

  const handleTogglePlayPause = useCallback(() => {
    handleVideoTogglePlayPause();
  }, [handleVideoTogglePlayPause]);

  useEffect(() => {
    if (!isPlaying) return undefined;
    if (
      typeof PerformanceObserver === "undefined" ||
      !PerformanceObserver.supportedEntryTypes?.includes("longtask")
    ) {
      return undefined;
    }
    const observer = new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) {
        if (entry.duration < 80) continue;
        console.info(
          `[NarrationPerf][LongTask] duration_ms=${entry.duration.toFixed(1)} start_ms=${entry.startTime.toFixed(1)} name=${entry.name || ""}`,
        );
      }
    });
    try {
      observer.observe({ entryTypes: ["longtask"] });
    } catch {
      return undefined;
    }
    return () => observer.disconnect();
  }, [isPlaying]);

  // FPS of the most-recent recording (set on stop, cleared when a different project loads).
  const [lastCaptureFps, setLastCaptureFps] = useState<number | null>(null);
  const [pendingAutoSubtitleProjectId, setPendingAutoSubtitleProjectId] = useState<string | null>(null);
  const [pendingAutoSubtitleArmed, setPendingAutoSubtitleArmed] = useState(false);

  const exportHook = useExport({
    videoRef,
    webcamVideoRef,
    canvasRef,
    tempCanvasRef,
    audioRef,
    micAudioRef,
    segment,
    backgroundConfig,
    webcamConfig,
    isRecording,
    isBatchEditing: isBatching,
    mousePositions,
    audioFilePath,
    micAudioFilePath: micAudioFilePath || currentRawMicAudioPath,
    webcamVideoFilePath: webcamVideoFilePath || currentRawWebcamVideoPath,
    videoFilePath,
    videoFilePathOwnerUrl,
    rawVideoPath: currentRawVideoPath,
    savedRawVideoPath: lastRawSavedPath,
    currentVideo,
    lastCaptureFps,
    composition,
    getLatestComposition: () => currentProjectDataRef.current?.composition ?? composition,
    currentProjectId: projects.currentProjectId,
    resolveClipExportSourcePath,
    resolveClipExportMicAudioPath,
    resolveClipExportWebcamPath,
  });

  const resolveCurrentExportSourcePath = useCallback((): string => {
    const directRecordingPath =
      currentVideo === videoFilePathOwnerUrl
        ? videoFilePath
        : "";
    return (
      directRecordingPath ||
      currentRawVideoPath ||
      lastRawSavedPath ||
      ""
    ).trim();
  }, [
    currentRawVideoPath,
    currentVideo,
    lastRawSavedPath,
    videoFilePath,
    videoFilePathOwnerUrl,
  ]);

  const audioDownloadHook = useAudioDownload({
    videoRef,
    segment,
    sourceVideoPath: resolveCurrentExportSourcePath(),
    micAudioPath: micAudioFilePath || currentRawMicAudioPath,
    composition,
    getLatestComposition: () => currentProjectDataRef.current?.composition ?? composition,
    resolveClipExportSourcePath,
    resolveClipExportMicAudioPath,
  });

  const {
    editingKeyframeId,
    setEditingKeyframeId,
    zoomFactor,
    setZoomFactor,
    handleDeleteKeyframe,
    throttledUpdateZoom,
    editingTextId,
    setEditingTextId,
    handleAddText,
    handleDeleteText,
    editingPointerId,
    setEditingPointerId,
    handleSmartPointerHiding,
    handleAddPointerSegment,
    handleDeletePointerSegment,
    handleAutoZoom,
    autoZoomConfig,
    handleAutoZoomConfigChange,
    getAutoCanvasSelectionConfig,
    customCanvasBaseDimensions,
    handleActivateCustomCanvas,
    handleApplyCanvasRatioPreset,
    handleCancelCrop,
    handleApplyCrop,
    handleToggleCrop,
    hasAppliedCrop,
    editingKeystrokeSegmentId,
    setEditingKeystrokeSegmentId,
    isKeystrokeOverlaySelected,
    setIsKeystrokeOverlaySelected,
    setIsKeystrokeResizeHandleHover,
    setIsKeystrokeResizeDragging,
    getKeystrokeTimelineDuration,
    keystrokeOverlayEditFrame,
    handleAddKeystrokeSegment,
    handleDeleteKeystrokeSegment,
    handleToggleKeystrokeMode,
    handleKeystrokeDelayChange,
    setIsPreviewDragging,
    seekIndicatorKey,
    setSeekIndicatorKey,
    seekIndicatorDir,
    setSeekIndicatorDir,
    previewCursorClass,
    handlePreviewMouseDown,
  } = useEditorTools({
    segment,
    setSegment,
    currentTime,
    duration,
    backgroundConfig,
    activePanel,
    setActivePanel,
    videoRef,
    isVideoReady,
    mousePositions,
    currentProjectId: projects.currentProjectId,
    loadProjects: projects.loadProjects,
    renderFrame,
    setBackgroundConfig,
    composition,
    activeClipId,
    isCropping,
    setIsCropping,
    isPlaying,
    handleTogglePlayPause,
    currentVideo,
    canvasRef,
    previewContainerRef,
    beginBatch,
    commitBatch,
  });
  const handleSelectedTextIdsChange = useCallback((ids: string[]) => {
    selectedTextIdsRef.current = ids;
  }, []);
  const handleSelectedSubtitleIdsChange = useCallback((ids: string[]) => {
    selectedSubtitleIdsRef.current = ids;
  }, []);
  const handleOverlayDragMove = useCallback((moves: Array<{ kind: 'text' | 'subtitle'; id: string; x: number; y: number }>) => {
    const liveSegment = segmentRef.current;
    if (!liveSegment || moves.length === 0) return;

    const textMoves = new Map<string, { x: number; y: number }>();
    const subtitleMoves = new Map<string, { x: number; y: number }>();
    for (const move of moves) {
      if (move.kind === 'subtitle') {
        subtitleMoves.set(move.id, { x: move.x, y: move.y });
      } else {
        textMoves.set(move.id, { x: move.x, y: move.y });
      }
    }

    let nextSegment = liveSegment;
    if (textMoves.size > 0) {
      nextSegment = {
        ...nextSegment,
        textSegments: (nextSegment.textSegments ?? []).map((text) => {
          const move = textMoves.get(text.id);
          return move
            ? {
                ...text,
                style: {
                  ...text.style,
                  x: move.x,
                  y: move.y,
                },
              }
            : text;
        }),
      };
    }

    if (subtitleMoves.size > 0) {
      nextSegment = updateSubtitleStylesAcrossTracks(
        nextSegment,
        new Set(subtitleMoves.keys()),
        (subtitle) => {
          const move = subtitleMoves.get(subtitle.id);
          return move
            ? {
                ...subtitle,
                style: {
                  ...subtitle.style,
                  x: move.x,
                  y: move.y,
                },
              }
            : subtitle;
        },
      );
    }

    setSegment(nextSegment);
  }, [setSegment]);
  const {
    editingSubtitleId,
    setEditingSubtitleId,
    subtitleSource,
    setSubtitleSource,
    subtitleMethod,
    setSubtitleMethod,
    subtitleMethodCapabilities,
    canUseSelectedSubtitleMethod,
    selectedSubtitleMethodReason,
    subtitleLanguageHint,
    setSubtitleLanguageHint,
    subtitleGeminiPrompt,
    setSubtitleGeminiPrompt,
    subtitleGroqVocabulary,
    setSubtitleGroqVocabulary,
    autoSplitSubtitles,
    setAutoSplitSubtitles,
    autoSplitMaxUnits,
    setAutoSplitMaxUnits,
    isGeneratingSubtitles,
    subtitleStatusMessage,
    subtitleGenerationIndicator,
    handleGenerateSubtitles,
    handleCancelSubtitleGeneration,
  } = useSubtitleGeneration({
    t: settings.t,
    projectResetKey: currentProjectData?.id ?? null,
    segment,
    setSegment: setSegment as (
      segment: VideoSegment | null | ((prev: VideoSegment | null) => VideoSegment | null),
      withHistory?: boolean,
    ) => void,
    composition,
    setComposition,
    activeClipId,
    currentRawVideoPath,
    currentRawMicAudioPath,
    duration,
    setActivePanel,
    persistProject: (opts) => persistRef.current?.(opts) ?? Promise.resolve(),
  });
  const handleDeleteSubtitle = useCallback(() => {
    if (!segment || !editingSubtitleId) return;
    beginBatch();
    setSegment(deleteSubtitleIdsAcrossTracks(segment, [editingSubtitleId]));
    setEditingSubtitleId(null);
    commitBatch();
  }, [beginBatch, commitBatch, editingSubtitleId, segment, setSegment, setEditingSubtitleId]);
  const isOverlayMode = projects.showProjectsDialog || isCropping;

  const {
    onProjectLoaded,
    handleLoadProjectFromGrid,
    handleToggleProjects,
    onStopRecording,
    selectedRecordingMode,
    setSelectedRecordingMode,
    captureSource,
    captureFps,
    recordingAudioSelection,
    isSelectingRecordingAudioApp,
    handleSelectMonitorCapture,
    handleSelectWindowCapture,
    handleToggleRecordingDeviceAudio,
    handleToggleRecordingMicAudio,
    handleSelectAllRecordingDeviceAudio,
    handleRequestRecordingAudioAppSelection,
    handleSelectWindowForRecording,
    handleStartRecording,
  } = useProjectLifecycle({
    persistRef,
    isProjectTransitionRef,
    isSwitchingCompositionClipRef,
    canvasRef,
    previewContainerRef,
    restoreImageRef,
    projectsPreviewTargetSnapshotRef,
    currentProjectId: projects.currentProjectId,
    projects: {
      projects: projects.projects,
      loadProjects: projects.loadProjects,
      setCurrentProjectId: projects.setCurrentProjectId,
    },
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
  // Wire onProjectLoaded into the ref so useProjects can always call the latest implementation.
  onProjectLoadedRef.current = onProjectLoaded;

  const mediaRecoveryInFlightRef = useRef(false);
  useEffect(() => {
    const handleMediaPipelineReset = (event: Event) => {
      const detail = (event as CustomEvent<{ reason?: string; delayMs?: number }>).detail;
      if (mediaRecoveryInFlightRef.current) return;
      const projectId = projects.currentProjectId;
      if (!projectId) {
        console.log("[ScreenRecord][MediaReset] project reload skipped: no active project");
        return;
      }
      mediaRecoveryInFlightRef.current = true;
      const resumeTime = currentTime;
      const shouldResume = isPlaying;
      console.log(
        `[ScreenRecord][MediaReset] project reload start project=${projectId} `
        + `reason=${detail?.reason ?? "unknown"} delay=${detail?.delayMs ?? "unknown"}ms `
        + `t=${resumeTime.toFixed(3)} playing=${shouldResume}`,
      );
      void (async () => {
        try {
          const project = currentProjectDataRef.current;
          if (!project || project.id !== projectId) return;
          let nextVideoUrl: string | undefined;
          if (currentRawVideoPath) {
            nextVideoUrl = await videoControllerRef.current?.loadVideo({
              videoUrl: await getMediaServerUrl(currentRawVideoPath),
              initialTime: resumeTime,
              debugLabel: "media-reset",
            });
          } else if (project.videoBlob) {
            nextVideoUrl = await videoControllerRef.current?.loadVideo({
              videoBlob: project.videoBlob,
              initialTime: resumeTime,
              debugLabel: "media-reset",
            });
          }
          if (nextVideoUrl) {
            setCurrentVideo((previous) => {
              if (previous?.startsWith("blob:") && previous !== nextVideoUrl) {
                URL.revokeObjectURL(previous);
              }
              return nextVideoUrl;
            });
          }
          let nextAudioUrl: string | undefined;
          if (currentRawVideoPath && segment?.deviceAudioAvailable !== false) {
            nextAudioUrl = await videoControllerRef.current?.loadDeviceAudio({
              audioUrl: await getMediaServerUrl(currentRawVideoPath),
            });
          } else if (project.audioBlob) {
            nextAudioUrl = await videoControllerRef.current?.loadDeviceAudio({
              audioBlob: project.audioBlob,
            });
          }
          if (nextAudioUrl) {
            setCurrentAudio((previous) => {
              if (previous?.startsWith("blob:") && previous !== nextAudioUrl) {
                URL.revokeObjectURL(previous);
              }
              return nextAudioUrl;
            });
          }
          let nextMicAudioUrl: string | undefined;
          if (currentRawMicAudioPath) {
            nextMicAudioUrl = await videoControllerRef.current?.loadMicAudio({
              audioUrl: await getMediaServerUrl(currentRawMicAudioPath),
            });
          } else if (project.micAudioBlob) {
            nextMicAudioUrl = await videoControllerRef.current?.loadMicAudio({
              audioBlob: project.micAudioBlob,
            });
          }
          if (nextMicAudioUrl) {
            setCurrentMicAudio((previous) => {
              if (previous?.startsWith("blob:") && previous !== nextMicAudioUrl) {
                URL.revokeObjectURL(previous);
              }
              return nextMicAudioUrl;
            });
          }
          let nextWebcamUrl: string | undefined;
          if (currentRawWebcamVideoPath) {
            nextWebcamUrl = await videoControllerRef.current?.loadWebcamVideo({
              videoUrl: await getMediaServerUrl(currentRawWebcamVideoPath),
            });
          } else if (project.webcamBlob) {
            nextWebcamUrl = await videoControllerRef.current?.loadWebcamVideo({
              videoBlob: project.webcamBlob,
            });
          }
          if (nextWebcamUrl) {
            setCurrentWebcamVideo((previous) => {
              if (previous?.startsWith("blob:") && previous !== nextWebcamUrl) {
                URL.revokeObjectURL(previous);
              }
              return nextWebcamUrl;
            });
          }
          setPreviewAudioResetKey((key) => key + 1);
          requestAnimationFrame(() => {
            seek(resumeTime);
            if (shouldResume) {
              window.setTimeout(() => videoControllerRef.current?.play(), 250);
            }
          });
          console.log("[ScreenRecord][MediaReset] project reload complete");
        } catch (error) {
          console.warn("[ScreenRecord][MediaReset] project reload failed", error);
        } finally {
          window.setTimeout(() => {
            mediaRecoveryInFlightRef.current = false;
          }, 5000);
        }
      })();
    };
    window.addEventListener("sr-reset-media-pipeline", handleMediaPipelineReset);
    return () => {
      window.removeEventListener("sr-reset-media-pipeline", handleMediaPipelineReset);
    };
  }, [
    currentTime,
    currentRawMicAudioPath,
    currentRawVideoPath,
    currentRawWebcamVideoPath,
    isPlaying,
    projects,
    seek,
    segment?.deviceAudioAvailable,
    setCurrentAudio,
    setCurrentMicAudio,
    setCurrentVideo,
    setCurrentWebcamVideo,
    videoControllerRef,
  ]);

  // Video import (external/non-recorded videos)
  const { isImporting, importVideo, importVideoPath } = useVideoImport({
    onProjectCreated: async (project) => {
      // Close projects dialog first (if open), then load the imported project directly.
      // Don't use handleLoadProjectFromGrid — it activates the interaction shield
      // which requires the FLIP animation to release. Import bypasses the FLIP.
      projects.setShowProjectsDialog(false);
      await projects.loadProjects();
      await projects.handleLoadProject(project.id);
    },
  });

  // Auto-pick the 'audio' subtitle source when a generated silent video is
  // only backing an imported audio timeline.
  useEffect(() => {
    if (
      composition?.placeholderVideoForAudio &&
      (composition.audioSegments?.length ?? 0) > 0
    ) {
      if (subtitleSource !== "audio" && !subtitleSource.startsWith("audio:")) {
        setSubtitleSource("audio");
      }
    }
  }, [
    composition?.placeholderVideoForAudio,
    composition?.audioSegments,
    subtitleSource,
    setSubtitleSource,
  ]);

  const applyCurrentComposition = useCallback(
    (nextComposition: ProjectComposition, reason: string) => {
      setComposition(nextComposition);
      const currentProject = currentProjectDataRef.current;
      if (currentProject) {
        currentProjectDataRef.current = {
          ...currentProject,
          composition: nextComposition,
        };
      }
      setCurrentProjectData((prev) =>
        prev ? { ...prev, composition: nextComposition } : prev,
      );

      const projectId =
        currentProjectIdRef.current ??
        currentProjectDataRef.current?.id ??
        projects.currentProjectId ??
        null;
      if (!projectId) {
        void logToHost(`[AudioImport][Frontend] skip composition persist reason="${reason}" no-project`);
        return;
      }

      const persistTask = compositionPersistChainRef.current
        .catch(() => undefined)
        .then(() => projectManager.updateProject(projectId, { composition: nextComposition }));
      compositionPersistChainRef.current = persistTask;
      void persistTask
        .then(() => projects.loadProjects())
        .catch((error) => {
          console.warn("[AudioImport] Failed to persist composition", error);
          void logToHost(
            `[AudioImport][Frontend] composition persist failed reason="${reason}" project="${projectId}" error="${String(error)}"`,
          );
        });
    },
    [projects.currentProjectId, projects.loadProjects],
  );

  const updateCurrentMusicSegments = useCallback(
    (
      updater: (segments: ImportedAudioSegment[]) => ImportedAudioSegment[],
      reason: string,
      options: { persist: boolean } = { persist: false },
    ) => {
      const baseComposition = currentProjectDataRef.current?.composition ?? composition ?? null;
      if (!baseComposition) {
        void logToHost(`[AudioImport][Frontend] skip audio update reason="${reason}" no-composition`);
        return;
      }

      const nextComposition: ProjectComposition = {
        ...baseComposition,
        audioSegments: updater(baseComposition.audioSegments ?? []),
      };

      if (options.persist) {
        applyCurrentComposition(nextComposition, reason);
        return;
      }

      setCompositionSilently(nextComposition);
      const currentProject = currentProjectDataRef.current;
      if (currentProject) {
        currentProjectDataRef.current = {
          ...currentProject,
          composition: nextComposition,
        };
      }
    },
    [applyCurrentComposition, composition, setCompositionSilently],
  );

  const persistCurrentComposition = useCallback(
    (reason: string) => {
      const currentComposition =
        currentProjectDataRef.current?.composition ?? composition ?? null;
      if (!currentComposition) {
        void logToHost(`[AudioImport][Frontend] skip composition persist reason="${reason}" no-composition`);
        return;
      }
      applyCurrentComposition(currentComposition, reason);
    },
    [applyCurrentComposition, composition],
  );

  const updateCurrentNarrationSegments = useCallback(
    (
      updater: (segments: NarrationSegment[]) => NarrationSegment[],
      reason: string,
      options: { persist: boolean } = { persist: false },
    ) => {
      const baseComposition = currentProjectDataRef.current?.composition ?? composition ?? null;
      if (!baseComposition) {
        void logToHost(`[Narration][Frontend] skip narration update reason="${reason}" no-composition`);
        return;
      }

      const nextComposition: ProjectComposition = {
        ...baseComposition,
        narrationSegments: updater(baseComposition.narrationSegments ?? []),
      };

      if (options.persist) {
        applyCurrentComposition(nextComposition, reason);
        return;
      }

      setCompositionSilently(nextComposition);
      const currentProject = currentProjectDataRef.current;
      if (currentProject) {
        currentProjectDataRef.current = {
          ...currentProject,
          composition: nextComposition,
        };
      }
    },
    [applyCurrentComposition, composition, setCompositionSilently],
  );

  const applyNarrationAudioSegments = useCallback(
    (segments: NarrationSegment[], replaceSubtitleIds: string[]) => {
      const perfStart = performance.now();
      const traceId = ++narrationApplyPerfSeqRef.current;
      const incomingBatchId = segments[0]?.narrationBatchId ?? null;
      const incomingIds = segments.map((segment) => segment.id);
      const wasPlaying = Boolean(videoRef.current && !videoRef.current.paused);
      const beforeCount =
        currentProjectDataRef.current?.composition?.narrationSegments?.length ??
        composition?.narrationSegments?.length ??
        0;
      const baseComposition =
        currentProjectDataRef.current?.composition ?? composition ?? null;
      if (!baseComposition) {
        void logToHost('[Narration][Frontend] skip apply no-composition');
        return;
      }

      const replaceSet = new Set(replaceSubtitleIds);
      const nextNarrationSegments = [
        ...(baseComposition.narrationSegments ?? []).filter((segment) => {
          const sourceIds = segment.sourceSubtitleIds?.length
            ? segment.sourceSubtitleIds
            : segment.sourceSubtitleId
              ? [segment.sourceSubtitleId]
              : [];
          if (sourceIds.length === 0) return true;
          return !sourceIds.some((id) => replaceSet.has(id));
        }),
        ...segments,
      ].sort((left, right) => left.startTime - right.startTime);

      let nextComposition: ProjectComposition = {
        ...baseComposition,
        narrationSegments: nextNarrationSegments,
      };
      let nextSegment = segmentRef.current;
      let nextDuration = duration;

      if (isPlaceholderBackedProject && nextSegment) {
        nextDuration = Math.max(
          duration,
          nextSegment.trimEnd,
          getTimelineContentEnd(
            nextSegment,
            baseComposition.audioSegments,
            nextNarrationSegments,
          ),
          1,
        );
        nextSegment = resizeSegmentDuration(nextSegment, nextDuration);
        nextComposition = resizeCompositionRootDuration(
          nextComposition,
          nextSegment,
          nextDuration,
        ) ?? nextComposition;
      }

      currentProjectDataRef.current = currentProjectDataRef.current
        ? {
            ...currentProjectDataRef.current,
            duration: Math.max(currentProjectDataRef.current.duration ?? 0, nextDuration),
            segment: nextSegment ?? currentProjectDataRef.current.segment,
            composition: nextComposition,
          }
        : currentProjectDataRef.current;

      if (wasPlaying && (segments.length > 0 || replaceSubtitleIds.length > 0)) {
        const projectId =
          currentProjectIdRef.current ??
          currentProjectDataRef.current?.id ??
          projects.currentProjectId ??
          null;
        applyLiveNarrationSegments(projectId, segments, replaceSubtitleIds);
        hasDeferredNarrationEditorFlushRef.current = true;
        const syncMs = performance.now() - perfStart;
        window.requestAnimationFrame(() => {
          window.requestAnimationFrame(() => {
            const twoFrameMs = performance.now() - perfStart;
            console.info(
              `[NarrationPerf][ApplyLive] trace=${traceId} job=${incomingBatchId ?? ""} incoming=${segments.length} replace=${replaceSubtitleIds.length} before=${beforeCount} after=${nextNarrationSegments.length} playing=1 sync_ms=${syncMs.toFixed(1)} two_frame_ms=${twoFrameMs.toFixed(1)} ids=${incomingIds.slice(0, 2).join(",")}`,
            );
          });
        });
        return;
      }

      const shouldResizeSegment = Boolean(nextSegment && nextDuration > duration);
      if (shouldResizeSegment && nextSegment) {
        editorHistory.replaceSnapshot({ segment: nextSegment });
        startTransition(() => {
          rawSetSegment(nextSegment);
          setEditorPreviewDuration(nextDuration);
        });
      }
      editorHistory.replaceSnapshot({ composition: nextComposition });
      startTransition(() => {
        rawSetComposition(nextComposition);
      });

      const syncMs = performance.now() - perfStart;
      window.requestAnimationFrame(() => {
        window.requestAnimationFrame(() => {
          const twoFrameMs = performance.now() - perfStart;
          if (syncMs < 12 && twoFrameMs < 80 && !wasPlaying) return;
          const activeAudioElements = document.querySelectorAll(".imported-audio-element").length;
          // Targeted WebView-only performance trace: logs only narration arrivals
          // that are likely to affect playback or happen during playback.
          console.info(
            `[NarrationPerf][Apply] trace=${traceId} job=${incomingBatchId ?? ""} incoming=${segments.length} replace=${replaceSubtitleIds.length} before=${beforeCount} after=${nextNarrationSegments.length} playing=${wasPlaying ? 1 : 0} sync_ms=${syncMs.toFixed(1)} two_frame_ms=${twoFrameMs.toFixed(1)} audio_el=${activeAudioElements} project_state=0 placeholder=${isPlaceholderBackedProject ? 1 : 0} resized=${nextDuration > duration ? 1 : 0} ids=${incomingIds.slice(0, 2).join(",")}`,
          );
        });
      });
    },
    [
      composition,
      duration,
      editorHistory,
      isPlaceholderBackedProject,
      setEditorPreviewDuration,
      videoRef,
    ],
  );

  const flushDeferredNarrationEditorState = useCallback((reason: string) => {
    if (!hasDeferredNarrationEditorFlushRef.current) return;
    const latestComposition = currentProjectDataRef.current?.composition ?? null;
    const latestSegment = currentProjectDataRef.current?.segment ?? null;
    const latestDuration = currentProjectDataRef.current?.duration ?? null;
    const projectId =
      currentProjectIdRef.current ??
      currentProjectDataRef.current?.id ??
      projects.currentProjectId ??
      null;
    if (!latestComposition) return;
    hasDeferredNarrationEditorFlushRef.current = false;
    const startedAt = performance.now();
    if (latestSegment && latestDuration && latestDuration > duration) {
      editorHistory.replaceSnapshot({ segment: latestSegment });
      startTransition(() => {
        rawSetSegment(latestSegment);
        setEditorPreviewDuration(latestDuration);
      });
    }
    editorHistory.replaceSnapshot({ composition: latestComposition });
    startTransition(() => {
      rawSetComposition(latestComposition);
    });
    window.requestAnimationFrame(() => {
      clearLiveNarrationSegments(projectId);
    });
    window.requestAnimationFrame(() => {
      window.requestAnimationFrame(() => {
        console.info(
          `[NarrationPerf][DeferredFlush] reason=${reason} narration=${latestComposition.narrationSegments?.length ?? 0} two_frame_ms=${(performance.now() - startedAt).toFixed(1)}`,
        );
      });
    });
  }, [duration, editorHistory, projects.currentProjectId, setEditorPreviewDuration]);

  useEffect(() => {
    if (isPlaying) return;
    flushDeferredNarrationEditorState("playback-stopped");
  }, [flushDeferredNarrationEditorState, isPlaying]);

  const persistTimelineWorkspaceState = useCallback(
    async (
      nextSegment: VideoSegment,
      nextComposition: ProjectComposition | null,
      nextDuration: number,
      reason: string,
      rawVideoPath?: string,
    ) => {
      setSegment(nextSegment);
      setEditorPreviewDuration(nextDuration);
      if (nextComposition) setComposition(nextComposition);
      if (rawVideoPath !== undefined) {
        handleEditorRawVideoPathChange(rawVideoPath);
      }

      const currentProject = currentProjectDataRef.current;
      if (currentProject) {
        const nextProject = {
          ...currentProject,
          duration: nextDuration,
          segment: nextSegment,
          composition: nextComposition ?? currentProject.composition,
          rawVideoPath: rawVideoPath ?? currentProject.rawVideoPath,
        };
        currentProjectDataRef.current = nextProject;
        setCurrentProjectData(nextProject);
      }

      const projectId =
        currentProjectIdRef.current ??
        currentProjectDataRef.current?.id ??
        projects.currentProjectId ??
        null;
      if (!projectId) {
        void logToHost(`[TimelineDuration] skip persist reason="${reason}" no-project`);
        return;
      }

      try {
        await projectManager.updateProject(projectId, {
          duration: nextDuration,
          segment: nextSegment,
          composition: nextComposition ?? undefined,
          ...(rawVideoPath !== undefined ? { rawVideoPath } : {}),
        });
        await projects.loadProjects();
      } catch (error) {
        console.warn(`[TimelineDuration] persist failed reason="${reason}"`, error);
      }
    },
    [
      handleEditorRawVideoPathChange,
      projects.currentProjectId,
      projects.loadProjects,
      setEditorPreviewDuration,
      setSegment,
    ],
  );

  const updatePlaceholderProjectDuration = useCallback(
    async (requestedDuration: number, reason: string) => {
      const currentSegment = segmentRef.current;
      if (!currentSegment) return;
      const currentComposition =
        currentProjectDataRef.current?.composition ?? composition ?? null;
      const contentEnd = getTimelineContentEnd(
        currentSegment,
        currentComposition?.audioSegments,
        currentComposition?.narrationSegments,
      );
      const nextDuration = Math.max(requestedDuration, contentEnd, 1);
      let nextSegment = resizeSegmentDuration(currentSegment, nextDuration);
      let nextComposition = resizeCompositionRootDuration(
        currentComposition,
        nextSegment,
        nextDuration,
      );
      let nextRawVideoPath: string | undefined;
      if (nextComposition) {
        const placeholder = await createAudioPlaceholderVideo(
          nextDuration,
          "placeholder-project-duration",
        );
        nextRawVideoPath = placeholder.path;
        nextSegment = { ...nextSegment, mediaMode: undefined };
        nextComposition = {
          ...nextComposition,
          timelineOnly: false,
          placeholderVideoForSubtitles:
            currentComposition?.placeholderVideoForSubtitles,
          placeholderVideoForAudio: currentComposition?.placeholderVideoForAudio,
          clips: nextComposition.clips.map((clip) =>
            clip.id === "root"
              ? {
                  ...clip,
                  duration: nextDuration,
                  segment: nextSegment,
                  rawVideoPath: placeholder.path,
                }
              : clip,
          ),
          globalSegment: nextComposition.globalSegment
            ? nextSegment
            : nextComposition.globalSegment,
        };
      }
      await persistTimelineWorkspaceState(
        nextSegment,
        nextComposition,
        nextDuration,
        reason,
        nextRawVideoPath,
      );
      if (nextRawVideoPath) {
        const mediaUrl = await getMediaServerUrl(nextRawVideoPath);
        const loadedUrl = await videoControllerRef.current?.loadVideo({
          videoUrl: mediaUrl,
          initialTime: Math.min(currentTime, nextDuration),
          debugLabel: "placeholder-project-duration",
        });
        setCurrentVideo(loadedUrl ?? mediaUrl);
      }
    },
    [composition, currentTime, persistTimelineWorkspaceState],
  );

  const finalizeNarrationAudioSegments = useCallback(async () => {
    if (isPlaceholderBackedProject && segmentRef.current) {
      await updatePlaceholderProjectDuration(
        segmentRef.current.trimEnd,
        'subtitle-narration-finalize',
      );
      return;
    }
    persistCurrentComposition('subtitle-narration-finalize');
  }, [
    isPlaceholderBackedProject,
    persistCurrentComposition,
    updatePlaceholderProjectDuration,
  ]);

  // Audio audio import — creates a silent-video-backed audio project when
  // nothing is open, otherwise appends to composition.audioSegments.
  const { isImporting: isImportingAudio, importAudio, importAudios, importAudioPaths } = useImportedAudioImport({
    getCurrentProjectId: () =>
      currentProjectIdRef.current ?? currentProjectDataRef.current?.id ?? null,
    onAttachToCurrentProject: async (segments) => {
      if (isPlaceholderBackedProject && segmentRef.current) {
        const baseComposition =
          currentProjectDataRef.current?.composition ?? composition ?? null;
        if (!baseComposition) return;
        const existingSegments = baseComposition.audioSegments ?? [];
        const appendStart = existingSegments.reduce((maxEnd, segment) => {
          const visibleDuration = Math.max(segment.outPoint - segment.inPoint, 0);
          return Math.max(maxEnd, segment.startTime + visibleDuration);
        }, 0);
        let cursor = appendStart;
        const placedSegments = segments.map((segment) => {
          const visibleDuration = Math.max(segment.outPoint - segment.inPoint, 0);
          const placed = { ...segment, startTime: cursor };
          cursor += visibleDuration;
          return placed;
        });
        const nextAudioSegments = [...existingSegments, ...placedSegments];
        const nextDuration = Math.max(
          duration,
          segmentRef.current.trimEnd,
          getTimelineContentEnd(
            segmentRef.current,
            nextAudioSegments,
            baseComposition.narrationSegments,
          ),
          1,
        );
        const nextSegment = {
          ...resizeSegmentDuration(segmentRef.current, nextDuration),
          mediaMode: undefined,
        };
        const placeholder = await createAudioPlaceholderVideo(
          nextDuration,
          "attach-audio-to-placeholder-project",
        );
        const nextComposition = {
          ...baseComposition,
          audioSegments: nextAudioSegments,
          timelineOnly: false,
          placeholderVideoForAudio: true,
          placeholderVideoForSubtitles: baseComposition.placeholderVideoForSubtitles,
          clips: baseComposition.clips.map((clip) =>
            clip.id === "root"
              ? {
                  ...clip,
                  duration: nextDuration,
                  segment: nextSegment,
                  rawVideoPath: placeholder.path,
                }
              : clip,
          ),
          globalSegment: baseComposition.globalSegment
            ? nextSegment
            : baseComposition.globalSegment,
        };
        await persistTimelineWorkspaceState(
          nextSegment,
          nextComposition,
          nextDuration,
          "attach-audio-to-placeholder-project",
          placeholder.path,
        );
        const mediaUrl = await getMediaServerUrl(placeholder.path);
        const loadedUrl = await videoControllerRef.current?.loadVideo({
          videoUrl: mediaUrl,
          initialTime: currentTime,
          debugLabel: "attach-audio-to-placeholder-project",
        });
        setCurrentVideo(loadedUrl ?? mediaUrl);
        setSubtitleSource("audio");
        return;
      }
      updateCurrentMusicSegments(
        (existingSegments) => {
          const appendStart = existingSegments.reduce((maxEnd, segment) => {
            const visibleDuration = Math.max(segment.outPoint - segment.inPoint, 0);
            return Math.max(maxEnd, segment.startTime + visibleDuration);
          }, 0);
          let cursor = appendStart;
          const placedSegments = segments.map((segment) => {
            const visibleDuration = Math.max(segment.outPoint - segment.inPoint, 0);
            const placed = { ...segment, startTime: cursor };
            cursor += visibleDuration;
            return placed;
          });
          return [...existingSegments, ...placedSegments];
        },
        "attach-audio-to-current-project",
        { persist: true },
      );
      if (composition?.placeholderVideoForAudio) {
        setSubtitleSource("audio");
      }
    },
    onCreateAudioProject: async (project) => {
      logToHost(`[AudioImport][Frontend] load project start id="${project.id}"`);
      projects.setShowProjectsDialog(false);
      await projects.loadProjects();
      logToHost(`[AudioImport][Frontend] project list refreshed id="${project.id}"`);
      await projects.handleLoadProject(project.id);
      currentProjectIdRef.current = project.id;
      if (project.composition) {
        setComposition(project.composition);
      }
      logToHost(`[AudioImport][Frontend] load project complete id="${project.id}"`);
    },
  });

  const {
    isImporting: isImportingSubtitle,
    importSubtitleFile,
    importSubtitlePayload,
  } = useSubtitleImport({
    segment,
    duration,
    getCurrentProjectId: () =>
      currentProjectIdRef.current ?? currentProjectDataRef.current?.id ?? null,
    setSegment,
    setActivePanel,
    setEditingSubtitleId,
    onImportedIntoCurrentProject: () => {
      selectedSubtitleIdsRef.current = [];
      selectedTextIdsRef.current = [];
    },
    onCreateSubtitleProject: async (project) => {
      logToHost(`[SubtitleImport][Frontend] load project start id="${project.id}"`);
      projects.setShowProjectsDialog(false);
      await projects.loadProjects();
      await projects.handleLoadProject(project.id);
      currentProjectIdRef.current = project.id;
      if (project.composition) {
        setComposition(project.composition);
      }
      logToHost(`[SubtitleImport][Frontend] load project complete id="${project.id}"`);
    },
  });

  // Drain pending audio-drop actions queued from the main SGT egui app
  // (file dropped onto the desktop tool, "Add to SGT Record" picked).
  useEffect(() => {
    let isDraining = false;
    const drainPendingAudioDropActions = () => {
      if (isDraining) return;
      isDraining = true;
      void (async () => {
        try {
          const actions = await invoke<{ path: string }[]>(
            "take_pending_audio_drop_actions",
            {},
          );
          const filePaths = actions
            .map((action) => action.path?.trim() ?? "")
            .filter(Boolean);
          if (filePaths.length > 0) {
            await importAudioPaths(filePaths);
          }
        } catch (error) {
          console.warn("[AudioDrop] Failed to drain pending audio actions", error);
        } finally {
          isDraining = false;
        }
      })();
    };

    window.addEventListener("sgt-audio-drop-pending", drainPendingAudioDropActions);
    drainPendingAudioDropActions();
    return () => {
      window.removeEventListener("sgt-audio-drop-pending", drainPendingAudioDropActions);
    };
  }, [importAudioPaths]);

  useEffect(() => {
    let isDraining = false;
    const drainPendingSubtitleDropActions = () => {
      if (isDraining) return;
      isDraining = true;
      void (async () => {
        try {
          const actions = await invoke<PendingSubtitleDropAction[]>(
            "take_pending_subtitle_drop_actions",
            {},
          );
          for (const action of actions) {
            const filePath = action.path?.trim();
            if (!filePath) continue;
            const result = await invoke<ReadSubtitleFilePathResult>(
              "read_subtitle_file_path",
              { path: filePath },
            );
            if (!result.content) continue;
            await importSubtitlePayload({
              fileName: result.fileName || filePath,
              content: result.content,
              format: result.format,
            });
            break;
          }
        } catch (error) {
          console.warn("[SubtitleDrop] Failed to drain pending subtitle actions", error);
        } finally {
          isDraining = false;
        }
      })();
    };

    window.addEventListener("sgt-subtitle-drop-pending", drainPendingSubtitleDropActions);
    drainPendingSubtitleDropActions();
    return () => {
      window.removeEventListener("sgt-subtitle-drop-pending", drainPendingSubtitleDropActions);
    };
  }, [importSubtitlePayload]);

  useEffect(() => {
    let isDraining = false;
    const drainPendingVideoDropActions = () => {
      if (isDraining) return;
      isDraining = true;
      void (async () => {
        try {
          const actions = await invoke<PendingVideoDropAction[]>(
            "take_pending_video_drop_actions",
            {},
          );
          for (const action of actions) {
            const filePath = action.path?.trim();
            if (!filePath) continue;
            const project = await importVideoPath(filePath);
            if (project && action.action === "generate-subtitles") {
              setPendingAutoSubtitleProjectId(project.id);
            }
          }
        } catch (error) {
          console.warn("[VideoDrop] Failed to drain pending video actions", error);
        } finally {
          isDraining = false;
        }
      })();
    };

    window.addEventListener("sgt-video-drop-pending", drainPendingVideoDropActions);
    drainPendingVideoDropActions();
    return () => {
      window.removeEventListener("sgt-video-drop-pending", drainPendingVideoDropActions);
    };
  }, [importVideoPath]);

  useEffect(() => {
    if (!pendingAutoSubtitleProjectId) return;
    if (projects.currentProjectId !== pendingAutoSubtitleProjectId) return;
    if (!currentRawVideoPath || isGeneratingSubtitles) return;
    setPendingAutoSubtitleProjectId(null);
    setActivePanel("subtitles");
    setSubtitleSource("video");
    setPendingAutoSubtitleArmed(true);
  }, [
    currentRawVideoPath,
    isGeneratingSubtitles,
    pendingAutoSubtitleProjectId,
    projects.currentProjectId,
    setSubtitleSource,
  ]);

  useEffect(() => {
    if (!pendingAutoSubtitleArmed || subtitleSource !== "video") return;
    setPendingAutoSubtitleArmed(false);
    window.setTimeout(() => {
      void handleGenerateSubtitles();
    }, 150);
  }, [handleGenerateSubtitles, pendingAutoSubtitleArmed, subtitleSource]);

  // App-level effects (persistence, background config cache, auto-save, toggle recording)
  useAppEffects({
    segment,
    segmentRef,
    backgroundConfig,
    currentProjectId: projects.currentProjectId,
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

  const handleCloseProject = useCallback(() => {
    if (isRecording || exportHook.isProcessing) return;
    [currentVideo, currentAudio, currentMicAudio, currentWebcamVideo].forEach((url) => {
      if (url?.startsWith("blob:")) URL.revokeObjectURL(url);
    });
    editorHistory.withoutHistory(() => {
      setCurrentVideo(null);
      setCurrentAudio(null);
      setCurrentMicAudio(null);
      setCurrentWebcamVideo(null);
      rawSetSegment(null);
      setThumbnails([]);
      setMousePositions([]);
      setCurrentTime(0);
      setPreviewDuration(0);
      setLoadedClipId(null);
      rawSetComposition(null);
      setCurrentProjectData(null);
    });
    projects.setCurrentProjectId(null);
    historyProjectResetRef.current = null;
    editorHistory.resetHistory({
      segment: null,
      composition: null,
      backgroundConfig,
      webcamConfig,
      duration: 0,
      currentRecordingMode,
      currentRawVideoPath,
      currentRawMicAudioPath,
      currentRawWebcamVideoPath,
    });
  }, [
    backgroundConfig,
    currentRawMicAudioPath,
    currentRawVideoPath,
    currentRawWebcamVideoPath,
    currentRecordingMode,
    isRecording,
    exportHook.isProcessing,
    currentVideo,
    currentAudio,
    currentMicAudio,
    currentWebcamVideo,
    setCurrentVideo,
    setCurrentAudio,
    setCurrentMicAudio,
    setCurrentWebcamVideo,
    editorHistory,
    setThumbnails,
    setMousePositions,
    setCurrentTime,
    setPreviewDuration,
    setLoadedClipId,
    projects,
    webcamConfig,
  ]);

  // Keyboard shortcuts, segment initializer, and keystroke drag
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
    handleOverlayDragMove,
    selectedTextIdsRef,
    selectedSubtitleIdsRef,
    beginBatch,
    commitBatch,
  });

  return (
    <SettingsContext.Provider value={settings}>
      <div className="app-container min-h-screen bg-[var(--surface)]">
        <DragDropOverlay
          disabled={isRecording || isImporting || isImportingAudio || isImportingSubtitle}
          onDropVideo={importVideo}
          onDropAudio={importAudio}
          onDropAudios={importAudios}
          onDropSubtitle={importSubtitleFile}
        />
        <ResizeBorders />
        <Header
          isRecording={isRecording}
          recordingDuration={recordingDuration}
          currentVideo={currentVideo}
          isProcessing={exportHook.isProcessing}
          hotkeys={hotkeys}
          onRemoveHotkey={handleRemoveHotkey}
          onOpenHotkeyDialog={openHotkeyDialog}
          recordingMode={selectedRecordingMode}
          onRecordingModeChange={setSelectedRecordingMode}
          recordingAudioSelection={recordingAudioSelection}
          isSelectingRecordingAudioApp={isSelectingRecordingAudioApp}
          onToggleRecordingDeviceAudio={handleToggleRecordingDeviceAudio}
          onToggleRecordingMicAudio={handleToggleRecordingMicAudio}
          onSelectAllRecordingDeviceAudio={handleSelectAllRecordingDeviceAudio}
          onRequestRecordingAudioAppSelection={handleRequestRecordingAudioAppSelection}
          rawButtonLabel={rawButtonSavedFlash ? settings.t.rawVideoSavedButton : settings.t.saveRawVideo}
          rawButtonPulse={currentRecordingMode === "withCursor"}
          rawButtonDisabled={!currentRawVideoPath && !lastRawSavedPath}
          onOpenRawVideoDialog={handleOpenRawVideoDialog}
          onExport={exportHook.handleExport}
          onOpenProjects={handleToggleProjects}
          projectsButtonDisabled={isProjectInteractionShieldVisible}
          onOpenCursorLab={() => { window.location.hash = "cursor-lab"; }}
          hideExport={isOverlayMode}
          hideRawVideo={projects.showProjectsDialog}
          captureSource={captureSource}
          captureFps={captureFps}
          monitors={monitors}
          onSelectMonitorCapture={handleSelectMonitorCapture}
          onSelectWindowCapture={handleSelectWindowCapture}
          showProjectsDialog={projects.showProjectsDialog}
          sequenceBreadcrumb={
            !isCropping && composition ? (
              <SequencePillChain
                composition={composition}
                activeClipId={activeClipId}
                spreadFromClipId={spreadFromClipId}
                onSelectClip={(clipId) => { void handleSelectSequenceClip(clipId); }}
                onInsertClip={handleOpenInsertProjectPicker}
                onRemoveClip={(clipId) => { void handleRemoveSequenceClip(clipId); }}
                onModeChange={(mode) => { void handleSequenceModeChange(mode); }}
                onCloseProject={handleCloseProject}
              />
            ) : undefined
          }
        />

        <EditorMain
          error={error}
          isOverlayMode={isOverlayMode}
          previewContainerRef={previewContainerRef}
          previewCursorClass={previewCursorClass}
          handlePreviewMouseDown={handlePreviewMouseDown}
          canvasRef={canvasRef}
          tempCanvasRef={tempCanvasRef}
          videoRef={videoRef}
          webcamVideoRef={webcamVideoRef}
          audioRef={audioRef}
          micAudioRef={micAudioRef}
          previousPreloadVideoRef={previousPreloadVideoRef}
          previousPreloadAudioRef={previousPreloadAudioRef}
          nextPreloadVideoRef={nextPreloadVideoRef}
          nextPreloadAudioRef={nextPreloadAudioRef}
          keystrokeOverlayEditFrame={keystrokeOverlayEditFrame}
          isKeystrokeOverlaySelected={isKeystrokeOverlaySelected}
          isDraggingKeystrokeOverlayRef={isDraggingKeystrokeOverlayRef}
          isResizingKeystrokeOverlayRef={isResizingKeystrokeOverlayRef}
          isBuffering={isBuffering}
          isPreviewPlaying={isPlaying}
          currentVideo={currentVideo}
          isTimelineOnly={isTimelineOnlyProject}
          isLoadingVideo={isLoadingVideo}
          loadingProgress={loadingProgress}
          isRecording={isRecording}
          recordingDuration={recordingDuration}
          isCropping={isCropping}
          backgroundConfig={backgroundConfig}
          setBackgroundConfig={setBackgroundConfig}
          beginBatch={beginBatch}
          commitBatch={commitBatch}
          setIsCanvasResizeDragging={setIsCanvasResizeDragging}
          seekIndicatorDir={seekIndicatorDir}
          seekIndicatorKey={seekIndicatorKey}
          audioResetKey={previewAudioResetKey}
          isPlaying={isPlaying}
          isProcessing={exportHook.isProcessing}
          isVideoReady={isVideoReady}
          hasAppliedCrop={hasAppliedCrop}
          currentTime={currentTime}
          duration={duration}
          handleTogglePlayPause={handleTogglePlayPause}
          handleToggleCrop={handleToggleCrop}
          onSetProjectDuration={
            isPlaceholderBackedProject
              ? (nextDuration) =>
                  void updatePlaceholderProjectDuration(
                    nextDuration,
                    "edit-project-duration",
                  )
              : undefined
          }
          customCanvasBaseDimensions={customCanvasBaseDimensions}
          getAutoCanvasSelectionConfig={getAutoCanvasSelectionConfig}
          handleActivateCustomCanvas={handleActivateCustomCanvas}
          handleApplyCanvasRatioPreset={handleApplyCanvasRatioPreset}
          isAutoCanvasDisabled={
            !!(composition && composition.clips.length > 1 && activeClipId &&
              composition.globalCanvasConfig?.canvasMode === 'auto' &&
              composition.globalCanvasConfig?.autoSourceClipId !== activeClipId)
          }
          segment={segment}
          setSegment={setSegment}
          composition={composition}
          setComposition={setComposition}
          handleToggleKeystrokeMode={handleToggleKeystrokeMode}
          handleKeystrokeDelayChange={handleKeystrokeDelayChange}
          mousePositionsLength={mousePositions.length}
          handleAutoZoom={handleAutoZoom}
          autoZoomConfig={autoZoomConfig}
          handleAutoZoomConfigChange={handleAutoZoomConfigChange}
          handleSmartPointerHiding={handleSmartPointerHiding}
          activePanel={activePanel}
          setActivePanel={setActivePanel}
          editingKeyframeId={editingKeyframeId}
          zoomFactor={zoomFactor}
          setZoomFactor={setZoomFactor}
          handleDeleteKeyframe={handleDeleteKeyframe}
          throttledUpdateZoom={throttledUpdateZoom}
          webcamConfig={webcamConfig}
          setWebcamConfig={setWebcamConfig}
          recentUploads={recentUploads}
          handleRemoveRecentUpload={handleRemoveRecentUpload}
          handleBackgroundUpload={handleBackgroundUpload}
          isBackgroundUploadProcessing={isBackgroundUploadProcessing}
          editingTextId={editingTextId}
          editingSubtitleId={editingSubtitleId}
          subtitleSource={subtitleSource}
          onSubtitleSourceChange={setSubtitleSource}
          subtitleMethod={subtitleMethod}
          onSubtitleMethodChange={setSubtitleMethod}
          subtitleMethodCapabilities={subtitleMethodCapabilities}
          canUseSelectedSubtitleMethod={canUseSelectedSubtitleMethod}
          selectedSubtitleMethodReason={selectedSubtitleMethodReason}
          subtitleLanguageHint={subtitleLanguageHint}
          onSubtitleLanguageHintChange={setSubtitleLanguageHint}
          subtitleGeminiPrompt={subtitleGeminiPrompt}
          onSubtitleGeminiPromptChange={setSubtitleGeminiPrompt}
          subtitleGroqVocabulary={subtitleGroqVocabulary}
          onSubtitleGroqVocabularyChange={setSubtitleGroqVocabulary}
          autoSplitSubtitles={autoSplitSubtitles}
          onAutoSplitSubtitlesChange={setAutoSplitSubtitles}
          autoSplitSubtitleMaxUnits={autoSplitMaxUnits}
          onAutoSplitSubtitleMaxUnitsChange={setAutoSplitMaxUnits}
          isGeneratingSubtitles={isGeneratingSubtitles}
          subtitleStatusMessage={subtitleStatusMessage}
          subtitleGenerationIndicator={subtitleGenerationIndicator}
          handleGenerateSubtitles={handleGenerateSubtitles}
          handleCancelSubtitleGeneration={handleCancelSubtitleGeneration}
          onApplyNarrationSegments={applyNarrationAudioSegments}
          onFinalizeNarrationSegments={finalizeNarrationAudioSegments}
          onSelectedTextIdsChange={handleSelectedTextIdsChange}
          onSelectedSubtitleIdsChange={handleSelectedSubtitleIdsChange}
          projectResetKey={currentProjectData?.id ?? null}
          currentRawVideoPath={currentRawVideoPath}
          currentRawMicAudioPath={currentRawMicAudioPath}
          currentProjectName={currentProjectData?.name ?? null}
          thumbnails={thumbnails}
          timelineRef={timelineRef}
          editingKeystrokeSegmentId={editingKeystrokeSegmentId}
          setCurrentTime={setCurrentTime}
          setEditingKeyframeId={setEditingKeyframeId}
          setEditingTextId={setEditingTextId}
          setEditingSubtitleId={setEditingSubtitleId}
          setEditingKeystrokeSegmentId={setEditingKeystrokeSegmentId}
          setEditingPointerId={setEditingPointerId}
          seek={seek}
          flushSeek={flushSeek}
          handleAddText={handleAddText}
          handleAddKeystrokeSegment={handleAddKeystrokeSegment}
          handleAddPointerSegment={handleAddPointerSegment}
          setTimelineCanvasWidthPx={setTimelineCanvasWidthPx}
          onPickImportedAudioFile={importAudio}
          onUpdateAudioSegment={(id, patch) => {
            if (isPlaceholderBackedProject && segmentRef.current) {
              const baseComposition =
                currentProjectDataRef.current?.composition ?? composition ?? null;
              if (!baseComposition) return;
              const nextAudioSegments = (baseComposition.audioSegments ?? []).map((segment) =>
                segment.id === id ? { ...segment, ...patch } : segment,
              );
              const nextDuration = Math.max(
                duration,
                segmentRef.current.trimEnd,
                getTimelineContentEnd(
                  segmentRef.current,
                  nextAudioSegments,
                  baseComposition.narrationSegments,
                ),
                1,
              );
              const nextSegment = resizeSegmentDuration(segmentRef.current, nextDuration);
              const nextComposition = {
                ...baseComposition,
                audioSegments: nextAudioSegments,
                timelineOnly: false,
                placeholderVideoForSubtitles: baseComposition.placeholderVideoForSubtitles,
                clips: baseComposition.clips.map((clip) =>
                  clip.id === "root"
                    ? { ...clip, duration: nextDuration, segment: nextSegment }
                    : clip,
                ),
                globalSegment: baseComposition.globalSegment
                  ? nextSegment
                  : baseComposition.globalSegment,
              };
              setSegment(nextSegment);
              setEditorPreviewDuration(nextDuration);
              setComposition(nextComposition);
              currentProjectDataRef.current = currentProjectDataRef.current
                ? {
                    ...currentProjectDataRef.current,
                    duration: nextDuration,
                    segment: nextSegment,
                    composition: nextComposition,
                  }
                : currentProjectDataRef.current;
              setCurrentProjectData((prev) =>
                prev
                  ? {
                      ...prev,
                      duration: nextDuration,
                      segment: nextSegment,
                      composition: nextComposition,
                    }
                  : prev,
              );
              return;
            }
            updateCurrentMusicSegments(
              (segments) =>
                segments.map((segment) =>
                  segment.id === id ? { ...segment, ...patch } : segment,
                ),
              "update-audio-segment",
              { persist: false },
            );
          }}
          onDeleteAudioSegments={(ids) => {
            const idSet = new Set(ids);
            updateCurrentMusicSegments(
              (segments) => segments.filter((segment) => !idSet.has(segment.id)),
              "delete-audio-segments",
              { persist: true },
            );
          }}
          onCommitAudioSegments={() => {
            if (isPlaceholderBackedProject && segmentRef.current) {
              void updatePlaceholderProjectDuration(
                segmentRef.current.trimEnd,
                "commit-audio-segment-edit",
              );
              return;
            }
            persistCurrentComposition("commit-audio-segment-edit");
          }}
          audioTrackVolumePoints={composition?.audioTrackVolumePoints}
          onUpdateAudioTrackVolumePoints={(points) => {
            const baseComposition = composition ?? currentProjectDataRef.current?.composition ?? null;
            if (!baseComposition) return;
            const next: ProjectComposition = { ...baseComposition, audioTrackVolumePoints: points };
            applyCurrentComposition(next, "update-audio-track-volume");
          }}
          narrationSegments={composition?.narrationSegments}
          onUpdateNarrationSegment={(id, patch) => {
            updateCurrentNarrationSegments(
              (segments) =>
                segments.map((segment) =>
                  segment.id === id ? { ...segment, ...patch } : segment,
                ),
              "update-narration-segment",
              { persist: false },
            );
          }}
          onDeleteNarrationSegments={(ids) => {
            const idSet = new Set(ids);
            updateCurrentNarrationSegments(
              (segments) => segments.filter((segment) => !idSet.has(segment.id)),
              "delete-narration-segments",
              { persist: true },
            );
          }}
          onCommitNarrationSegments={() => {
            if (isPlaceholderBackedProject && segmentRef.current) {
              void updatePlaceholderProjectDuration(
                segmentRef.current.trimEnd,
                "commit-narration-segment-edit",
              );
              return;
            }
            persistCurrentComposition("commit-narration-segment-edit");
          }}
          narrationTrackVolumePoints={composition?.narrationTrackVolumePoints}
          onUpdateNarrationTrackVolumePoints={(points) => {
            const baseComposition = composition ?? currentProjectDataRef.current?.composition ?? null;
            if (!baseComposition) return;
            const next: ProjectComposition = { ...baseComposition, narrationTrackVolumePoints: points };
            applyCurrentComposition(next, "update-narration-track-volume");
          }}
          onAudioTrackDownload={audioDownloadHook.openAudioDownloadDialog}
        />

        <EditorOverlays
          showProjectsDialog={projects.showProjectsDialog}
          projects={projects.projects}
          onBeginProjectOpen={beginProjectInteractionShield}
          onLoadProject={handleLoadProjectFromGrid}
          onProjectsChange={projects.loadProjects}
          currentProjectId={projects.currentProjectId}
          restoreImageRef={restoreImageRef}
          previewTargetSnapshotRef={projectsPreviewTargetSnapshotRef}
          projectPickerMode={projectPickerMode}
          setProjectPickerMode={setProjectPickerMode}
          setShowProjectsDialog={projects.setShowProjectsDialog}
          armProjectInteractionShieldRelease={armProjectInteractionShieldRelease}
          onPickProject={handlePickProjectForSequence}
          onImportVideo={importVideo}
          onImportAudio={importAudio}
          isProjectInteractionShieldVisible={isProjectInteractionShieldVisible}
          isCropping={isCropping}
          currentVideo={currentVideo}
          segment={segment}
          currentTime={currentTime}
          onCancelCrop={handleCancelCrop}
          onApplyCrop={handleApplyCrop}
          exportHook={exportHook}
          audioDownloadHook={audioDownloadHook}
          videoRef={videoRef}
          showWindowSelect={showWindowSelect}
          onCloseWindowSelect={() => setShowWindowSelect(false)}
          windows={windows}
          onSelectWindowForRecording={handleSelectWindowForRecording}
          isVideoReady={isVideoReady}
          showRawVideoDialog={showRawVideoDialog}
          onCloseRawVideoDialog={() => setShowRawVideoDialog(false)}
          lastRawSavedPath={lastRawSavedPath}
          rawAutoCopyEnabled={rawAutoCopyEnabled}
          isRawActionBusy={isRawActionBusy}
          onChangeRawSavedPath={setLastRawSavedPath}
          onToggleRawAutoCopy={handleToggleRawAutoCopy}
          onExportSuccessPathChange={async (newPath) => exportHook.setLastExportedPath(newPath)}
          showHotkeyDialog={showHotkeyDialog}
          onCloseHotkeyDialog={closeHotkeyDialog}
        />
      </div>
    </SettingsContext.Provider>
  );
}

export default App;
