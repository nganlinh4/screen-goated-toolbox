import { useEffect, useRef, useState } from "react";
import "./App.css";
import {
  BackgroundConfig, Project, ProjectComposition,
  VideoSegment, RecordingMode, WebcamConfig,
} from "@/types/video";

import { useHotkeys, useMonitors, useWindows } from "@/hooks/useAppHooks";
import { useProjects } from "@/hooks/useVideoState";
import { useMediaEngine } from "@/hooks/useMediaEngine";
import { getInitialBackgroundConfig } from "@/lib/appUtils";
import { useBackgroundManager } from "@/hooks/useBackgroundManager";
import { useEditorTools } from "@/hooks/useEditorTools";

import { type ActivePanel } from "@/components/sidepanel/index";
import { type ProjectsPreviewTargetSnapshot } from "@/components/ProjectsView";
import { SettingsContext, useSettingsProvider } from "@/hooks/useSettings";
import { useRawVideoHandler } from "@/hooks/useRawVideoHandler";
import { type PersistOptions } from "@/hooks/useSequenceComposition";
import { useAppSubtitleController } from "@/hooks/useAppSubtitleController";
import { useAppHistoryState } from "@/hooks/useAppHistoryState";
import { useAppOverlaySelection } from "@/hooks/useAppOverlaySelection";
import { useAppProjectHarness } from "@/hooks/useAppProjectHarness";
import { useAppTimelineControllers } from "@/hooks/useAppTimelineControllers";
import { useAppLateEffects } from "@/hooks/useAppLateEffects";
import { useAppCompositionExportController } from "@/hooks/useAppCompositionExportController";
import { useAppProjectLifecycleController } from "@/hooks/useAppProjectLifecycleController";
import { AppView } from "@/components/AppView";
import { cloneBackgroundConfig } from "@/lib/backgroundConfig";
import { cloneWebcamConfig, DEFAULT_WEBCAM_CONFIG } from "@/lib/webcam";
import { installFrontendPerfDiagnostics } from "@/lib/frontendPerfDiagnostics";

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
  const isDraggingKeystrokeOverlayRef = useRef(false);
  const isResizingKeystrokeOverlayRef = useRef(false);
  const [isCanvasResizeDragging, setIsCanvasResizeDragging] = useState(false);
  // Stable ref for onProjectLoaded — breaks circular dep between useClipMediaCache and useProjects
  const onProjectLoadedRef = useRef<(project: Project) => void>(null!);
  const currentProjectIdRef = useRef<string | null>(null);
  const currentProjectDataRef = useRef<Project | null>(null);
  const isPlayingRef = useRef(false);
  const pendingSilentSegmentRef = useRef<VideoSegment | null>(null);
  const pendingSilentSegmentTimerRef = useRef<number | null>(null);
  // Stable ref for persist callback — avoids cascading useEffect re-triggers
  const persistRef = useRef<((opts?: PersistOptions) => Promise<void>) | null>(null);
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

  const {
    undo,
    redo,
    canUndo,
    canRedo,
    isBatching,
    beginBatch,
    commitBatch,
    editorHistory,
    handleEditorRawVideoPathChange,
    setBackgroundConfig,
    setComposition,
    setCompositionSilently,
    setEditorPreviewDuration,
    setSegment,
    setSegmentSilently,
    setWebcamConfig,
  } = useAppHistoryState({
    backgroundConfig,
    composition,
    currentProjectDataRef,
    currentRawMicAudioPath,
    currentRawVideoPath,
    currentRawWebcamVideoPath,
    currentRecordingMode,
    duration,
    handleProjectRawVideoPathChange,
    isPlaying,
    isPlayingRef,
    pendingSilentSegmentRef,
    pendingSilentSegmentTimerRef,
    rawSetBackgroundConfig,
    rawSetComposition,
    rawSetCurrentRawMicAudioPath,
    rawSetCurrentRawVideoPath,
    rawSetCurrentRawWebcamVideoPath,
    rawSetCurrentRecordingMode,
    rawSetSegment,
    rawSetWebcamConfig,
    segment,
    segmentRef,
    setBackgroundConfigState,
    setCurrentProjectData,
    setLastRawSavedPath,
    setPreviewDuration,
    webcamConfig,
  });

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
  const historyProjectResetRef = useAppProjectHarness({
    composition, currentProjectData, currentProjectDataRef, currentProjectIdRef,
    currentRawMicAudioPath, currentRawVideoPath, currentRawWebcamVideoPath, currentRecordingMode,
    duration, editorHistory, handleProjectRawVideoPathChange, projects, rawSetComposition,
    rawSetCurrentRawMicAudioPath, rawSetCurrentRawWebcamVideoPath, rawSetSegment, rawSetWebcamConfig,
    segment, segmentRef, setBackgroundConfigState, setCurrentAudio, setCurrentMicAudio,
    setCurrentProjectData, setCurrentTime, setCurrentVideo, setCurrentWebcamVideo,
    setMousePositions, setPreviewDuration, setThumbnails,
  });

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
    activeClipId,
    handleOpenInsertProjectPicker,
    handlePickProjectForSequence,
    handleSelectSequenceClip,
    handleRemoveSequenceClip,
    handleSequenceModeChange,
    audioDownloadHook,
    exportHook,
    handleTogglePlayPause,
    setLastCaptureFps,
  } = useAppCompositionExportController({
    applyLoadedBackgroundConfig, audioFilePath, audioRef, backgroundConfig, backgroundMutationMetaRef,
    canvasRef, composition, currentAudio, currentMicAudio, currentProjectData, currentProjectDataRef,
    currentProjectId: projects.currentProjectId, currentRawMicAudioPath, currentRawVideoPath,
    currentRawWebcamVideoPath, currentRecordingMode, currentTime, currentVideo, currentWebcamVideo,
    duration, generateThumbnailsForSource, handleProjectRawVideoPathChange, invalidateThumbnails,
    isBatching, isCropping, isLoadingVideo, isPlaying, isProjectTransitionRef, isRecording,
    isVideoReady, lastRawSavedPath, micAudioFilePath, micAudioRef, mousePositions, persistRef,
    previewContainerRef, rawSetCurrentRawMicAudioPath, rawSetCurrentRawWebcamVideoPath,
    rawSetCurrentRecordingMode, rawSetWebcamConfig, segment, seek, setComposition,
    setCompositionSilently, setCurrentAudio, setCurrentMicAudio, setCurrentProjectData,
    setCurrentVideo, setCurrentWebcamVideo, setMousePositions, setPreviewDuration, setSegment,
    setShowProjectsDialog: projects.setShowProjectsDialog, setThumbnails,
    showProjectsDialog: projects.showProjectsDialog, tempCanvasRef, thumbnails, timelineCanvasWidthPx,
    togglePlayback, videoControllerRef, videoFilePath, videoFilePathOwnerUrl, videoRef, webcamConfig,
    webcamVideoFilePath, webcamVideoRef,
  });
  const [pendingAutoSubtitleProjectId, setPendingAutoSubtitleProjectId] = useState<string | null>(null);
  const [pendingAutoSubtitleArmed, setPendingAutoSubtitleArmed] = useState(false);

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
  const {
    selectedTextIdsRef,
    selectedSubtitleIdsRef,
    handleSelectedTextIdsChange,
    handleSelectedSubtitleIdsChange,
    handleOverlayDragMove,
  } = useAppOverlaySelection({
    segmentRef,
    setSegment,
  });
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
    handleDeleteSubtitle,
  } = useAppSubtitleController({
    subtitleOptions: {
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
      isSubtitlePanelActive: activePanel === "subtitles",
      setActivePanel,
      persistProject: (opts) => persistRef.current?.(opts) ?? Promise.resolve(),
    },
    segment,
    setSegment,
    beginBatch,
    commitBatch,
  });
  const isOverlayMode = projects.showProjectsDialog || isCropping;

  const {
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
  } = useAppProjectLifecycleController({
    abortEditorInteractions, applyLoadedBackgroundConfig, armProjectInteractionShieldRelease,
    backgroundConfig, beginProjectInteractionShield, canvasRef, clearClipMediaCaches,
    clipExportSourcePathCacheRef, clipExportWebcamPathCacheRef, clipLoadRequestSeqRef,
    composition, currentAudio, currentMicAudio, currentProjectData, currentProjectDataRef,
    currentProjectId: projects.currentProjectId, currentRawMicAudioPath, currentRawVideoPath,
    currentRawWebcamVideoPath, currentRecordingMode, currentTime, currentVideo, currentWebcamVideo,
    duration, exportHook, flashRawSavedButton, generateThumbnail, getMonitors, getWindows,
    handleProjectRawVideoPathChange, handleStopRecording, isPlaying, isProjectInteractionShieldVisible,
    isProjectTransitionRef, isRecording, isSwitchingCompositionClipRef, loadClipAssets,
    loadClipMediaIntoEditor, loadedClipId, monitors, mousePositions, onProjectLoadedRef,
    persistRef, previewContainerRef, projectInteractionBlockCleanupRef, projectInteractionShieldReleaseRef,
    projects, projectsPreviewTargetSnapshotRef, rawAutoCopyEnabled, rawSaveDir,
    rawSetCurrentRawMicAudioPath, rawSetCurrentRawWebcamVideoPath, rawSetCurrentRecordingMode,
    restoreImageRef, seek, segment, setComposition, setCurrentAudio, setCurrentMicAudio,
    setCurrentProjectData, setCurrentTime, setCurrentVideo, setCurrentWebcamVideo, setError,
    setIsProjectInteractionShieldVisible, setIsRawActionBusy, setLastCaptureFps, setLastRawSavedPath,
    setLoadedClipId, setMousePositions, setPreviewAudioResetKey, setPreviewDuration,
    setRawButtonSavedFlash, setShowRawVideoDialog, setShowWindowSelect, setSpreadFromClipId,
    setWebcamConfig, showWindowSelect, spreadAnimationTimerRef, startNewRecording, videoControllerRef,
    webcamConfig,
  });

  const {
    applyNarrationAudioSegments,
    finalizeNarrationAudioSegments,
    updatePlaceholderProjectDuration,
    handleCommitAudioSegments,
    handleCommitNarrationSegments,
    handleDeleteAudioSegments,
    handleDeleteNarrationSegments,
    handleUpdateAudioSegment,
    handleUpdateAudioTrackVolumePoints,
    handleUpdateNarrationSegment,
    handleUpdateNarrationTrackVolumePoints,
    importAudio,
    importAudios,
    importSubtitleFile,
    importVideo,
    isImporting,
    isImportingAudio,
    isImportingSubtitle,
  } = useAppTimelineControllers({
    composition, currentProjectDataRef, currentProjectId: projects.currentProjectId, currentProjectIdRef,
    currentRawVideoPath, currentTime, duration, editorHistory, handleEditorRawVideoPathChange,
    handleGenerateSubtitles, isGeneratingSubtitles, isPlaceholderBackedProject, isPlaying,
    loadProjects: projects.loadProjects, pendingAutoSubtitleArmed, pendingAutoSubtitleProjectId,
    projects, rawSetComposition, rawSetSegment, segment, segmentRef, selectedSubtitleIdsRef,
    selectedTextIdsRef, setActivePanel, setComposition, setCompositionSilently, setCurrentProjectData,
    setCurrentVideo, setEditorPreviewDuration, setEditingSubtitleId, setPendingAutoSubtitleArmed,
    setPendingAutoSubtitleProjectId, setSegment, setSubtitleSource, subtitleSource, videoControllerRef,
    videoRef,
  });

  const { handleCloseProject } = useAppLateEffects({
    backgroundConfig, beginBatch, canRedo, canUndo, canvasRef, commitBatch, composition,
    currentAudio, currentMicAudio, currentProjectId: projects.currentProjectId,
    currentRawMicAudioPath, currentRawVideoPath, currentRawWebcamVideoPath, currentRecordingMode,
    currentTime, currentVideo, currentWebcamVideo, duration, editingKeystrokeSegmentId,
    editingKeyframeId, editingPointerId, editingSubtitleId, editingTextId, editorHistory,
    exportHook, getKeystrokeTimelineDuration, handleDeleteKeystrokeSegment,
    handleDeletePointerSegment, handleDeleteSubtitle, handleDeleteText, handleOverlayDragMove,
    handleStartRecording, handleTogglePlayPause, historyProjectResetRef, isCropping,
    isDraggingKeystrokeOverlayRef, isRecording, isResizingKeystrokeOverlayRef, mousePositions,
    onStopRecording, persistRef, projects, rawSetComposition, rawSetSegment, redo, seek, segment,
    segmentRef, selectedSubtitleIdsRef, selectedTextIdsRef, setActivePanel, setCurrentAudio,
    setCurrentMicAudio, setCurrentProjectData, setCurrentTime, setCurrentVideo,
    setCurrentWebcamVideo, setEditingKeyframeId, setEditingSubtitleId, setEditingTextId,
    setIsKeystrokeOverlaySelected, setIsKeystrokeResizeDragging, setIsKeystrokeResizeHandleHover,
    setIsPreviewDragging, setLoadedClipId, setMousePositions, setPreviewDuration,
    setSeekIndicatorDir, setSeekIndicatorKey, setSegment, setThumbnails, showHotkeyDialog,
    showRawVideoDialog, tempCanvasRef, undo, videoRef, webcamConfig,
  });

  return (
    <SettingsContext.Provider value={settings}>
      <AppView {...{
        activeClipId, activePanel, applyNarrationAudioSegments, armProjectInteractionShieldRelease, audioDownloadHook, audioRef, autoSplitMaxUnits,
        autoSplitSubtitles, autoZoomConfig, backgroundConfig, beginBatch, beginProjectInteractionShield, canUseSelectedSubtitleMethod, canvasRef,
        captureFps, captureSource, closeHotkeyDialog, commitBatch, composition, currentProjectData, currentRawMicAudioPath, currentRawVideoPath,
        currentRecordingMode, currentTime, currentVideo, customCanvasBaseDimensions, duration, editingKeystrokeSegmentId, editingKeyframeId,
        editingSubtitleId, editingTextId, error, exportHook, finalizeNarrationAudioSegments, flushSeek, getAutoCanvasSelectionConfig,
        handleActivateCustomCanvas, handleAddKeystrokeSegment, handleAddPointerSegment, handleAddText, handleApplyCanvasRatioPreset,
        handleApplyCrop, handleAutoZoom, handleAutoZoomConfigChange, handleBackgroundUpload, handleCancelCrop, handleCancelSubtitleGeneration,
        handleCloseProject, handleCommitAudioSegments, handleCommitNarrationSegments, handleDeleteAudioSegments, handleDeleteKeyframe,
        handleDeleteNarrationSegments, handleGenerateSubtitles, handleKeystrokeDelayChange, handleLoadProjectFromGrid, handleOpenInsertProjectPicker,
        handleOpenRawVideoDialog, handlePickProjectForSequence, handlePreviewMouseDown, handleRemoveHotkey, handleRemoveRecentUpload,
        handleRemoveSequenceClip, handleRequestRecordingAudioAppSelection, handleSelectAllRecordingDeviceAudio, handleSelectMonitorCapture,
        handleSelectSequenceClip, handleSelectWindowCapture, handleSelectWindowForRecording, handleSelectedSubtitleIdsChange,
        handleSelectedTextIdsChange, handleSequenceModeChange, handleSmartPointerHiding, handleToggleCrop, handleToggleKeystrokeMode,
        handleTogglePlayPause, handleToggleProjects, handleToggleRawAutoCopy, handleToggleRecordingDeviceAudio, handleToggleRecordingMicAudio,
        handleUpdateAudioSegment, handleUpdateAudioTrackVolumePoints, handleUpdateNarrationSegment, handleUpdateNarrationTrackVolumePoints,
        hasAppliedCrop, hotkeys, importAudio, importAudios, importSubtitleFile, importVideo, isBackgroundUploadProcessing, isBuffering,
        isCropping, isDraggingKeystrokeOverlayRef, isGeneratingSubtitles, isImporting, isImportingAudio, isImportingSubtitle,
        isKeystrokeOverlaySelected, isLoadingVideo, isOverlayMode, isPlaceholderBackedProject, isPlaying, isProjectInteractionShieldVisible,
        isRawActionBusy, isRecording, isResizingKeystrokeOverlayRef, isSelectingRecordingAudioApp, isTimelineOnlyProject, isVideoReady,
        keystrokeOverlayEditFrame, lastRawSavedPath, loadingProgress, micAudioRef, monitors, mousePositions, nextPreloadAudioRef,
        nextPreloadVideoRef, openHotkeyDialog, previewAudioResetKey, previewContainerRef, previewCursorClass, previousPreloadAudioRef,
        previousPreloadVideoRef, projectPickerMode, projects, projectsPreviewTargetSnapshotRef, rawAutoCopyEnabled, rawButtonSavedFlash,
        recentUploads, recordingAudioSelection, recordingDuration, restoreImageRef, seek, seekIndicatorDir, seekIndicatorKey, segment,
        selectedRecordingMode, selectedSubtitleMethodReason, setActivePanel, setAutoSplitMaxUnits, setAutoSplitSubtitles, setBackgroundConfig,
        setComposition, setCurrentTime, setEditingKeystrokeSegmentId, setEditingKeyframeId, setEditingPointerId, setEditingSubtitleId,
        setEditingTextId, setIsCanvasResizeDragging, setLastRawSavedPath, setProjectPickerMode, setSegment, setSegmentSilently,
        setSelectedRecordingMode, setShowRawVideoDialog, setShowWindowSelect, setSubtitleGeminiPrompt, setSubtitleGroqVocabulary,
        setSubtitleLanguageHint, setSubtitleMethod, setSubtitleSource, setTimelineCanvasWidthPx, setWebcamConfig, setZoomFactor, settings,
        showHotkeyDialog, showRawVideoDialog, showWindowSelect, spreadFromClipId, subtitleGeminiPrompt, subtitleGenerationIndicator,
        subtitleGroqVocabulary, subtitleLanguageHint, subtitleMethod, subtitleMethodCapabilities, subtitleSource, subtitleStatusMessage,
        tempCanvasRef, throttledUpdateZoom, thumbnails, timelineRef, updatePlaceholderProjectDuration, videoRef, webcamConfig, webcamVideoRef,
        windows, zoomFactor,
      }} />
    </SettingsContext.Provider>
  );
}

export default App;
