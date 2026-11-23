import { useCallback, useEffect, useRef, useState } from "react";
import "./App.css";
import { readSubtitleSourceSnapshot } from "@/hooks/subtitleSourceOwnership";
import { beginNarrationProjectLoad, narrationProjectEpoch } from "@/hooks/narrationProjectOwnership";
import { importedProjectOwner } from "@/hooks/importProjectOwnership";
import { assertAudioImportWriteAllowed } from "@/lib/audioImportWriteFence";
import { useHotkeys, useMonitors } from "@/hooks/useAppHooks";
import { useProjects } from "@/hooks/useVideoState";
import { useMediaEngine } from "@/hooks/useMediaEngine";
import { useBackgroundSelection } from "@/hooks/useBackgroundSelection";
import { useBackgroundManager } from "@/hooks/useBackgroundManager";
import { useEditorTools } from "@/hooks/useEditorTools";
import { type ActivePanel } from "@/components/sidepanel/index";
import { type ProjectsPreviewTargetSnapshot } from "@/components/ProjectsView";
import { SettingsContext, useSettingsProvider } from "@/hooks/useSettings";
import { useRawVideoHandler } from "@/hooks/useRawVideoHandler";
import { useAppSubtitleController } from "@/hooks/useAppSubtitleController";
import { useAppHistoryState } from "@/hooks/useAppHistoryState";
import { useAppOverlaySelection } from "@/hooks/useAppOverlaySelection";
import { useAppProjectHarness } from "@/hooks/useAppProjectHarness";
import { useAppTimelineControllers } from "@/hooks/useAppTimelineControllers";
import { useAppLateEffects } from "@/hooks/useAppLateEffects";
import { useAppCompositionExportController } from "@/hooks/useAppCompositionExportController";
import { useAppProjectLifecycleController } from "@/hooks/useAppProjectLifecycleController";
import { useProjectEditorState } from "@/hooks/useProjectEditorState";
import { AppView } from "@/components/AppView";
import { UserNotificationRegion } from "@/components/UserNotificationRegion";
import { installFrontendPerfDiagnostics } from "@/lib/frontendPerfDiagnostics";
import type { BackgroundUploadTarget } from "@/hooks/useBackgroundUpload";
import { resolveBackgroundUploadTarget } from "@/lib/backgroundUploadTarget";
function App() {
  useEffect(() => {
    installFrontendPerfDiagnostics();
  }, []);
  const settings = useSettingsProvider();
  const {
    composition, currentProjectData, currentProjectDataRef, currentProjectIdRef,
    currentRawMicAudioPath, currentRawMicAudioPathRef, currentRawWebcamVideoPath, currentRecordingMode,
    isPlaceholderBackedProject, isPlayingRef, isProjectTransitionRef,
    isTimelineOnlyProject, onProjectLoadedRef, pendingSilentSegmentRef,
    pendingSilentSegmentTimerRef, persistRef, rawSetComposition,
    rawSetCurrentRawMicAudioPath, rawSetCurrentRawWebcamVideoPath,
    rawSetCurrentRecordingMode, rawSetSegment, rawSetWebcamConfig, segment,
    segmentRef, setCurrentProjectData, webcamConfig, webcamConfigRef,
  } = useProjectEditorState();
  const [activePanel, setActivePanel] = useState<ActivePanel>("background");
  const [isCropping, setIsCropping] = useState(false);
  const { backgroundConfig, backgroundConfigRef, backgroundSelectionRevisionRef, setBackgroundConfigState } =
    useBackgroundSelection();
  const uploadTargetResolverRef = useRef<() => BackgroundUploadTarget | null>(() => null);
  const applyUploadedBackgroundRef = useRef<(url: string) => void>(() => {});
  const {
    currentRawVideoPath, currentRawVideoPathRef,
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
  const [timelineCanvasWidthPx, setTimelineCanvasWidthPx] = useState(0);
  const [previewAudioResetKey, setPreviewAudioResetKey] = useState(0);
  const timelineRef = useRef<HTMLDivElement>(null);
  const previewContainerRef = useRef<HTMLDivElement>(null);
  const restoreImageRef = useRef<string | null>(null);
  const projectsPreviewTargetSnapshotRef = useRef<ProjectsPreviewTargetSnapshot | null>(null);
  const isDraggingKeystrokeOverlayRef = useRef(false);
  const isResizingKeystrokeOverlayRef = useRef(false);
  const [isCanvasResizeDragging, setIsCanvasResizeDragging] = useState(false);

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
    composition,
    setBackgroundConfigState,
    isProjectTransitionRef,
    getUploadTarget: () => uploadTargetResolverRef.current(),
    applyUploadedBackground: (url) => applyUploadedBackgroundRef.current(url),
  });

  const {
    hotkeys,
    showHotkeyDialog,
    handleRemoveHotkey,
    openHotkeyDialog,
    closeHotkeyDialog,
  } = useHotkeys();
  const { monitors, getMonitors } = useMonitors();

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
    renderFrame, bindSubtitleClipOwner,
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
    segment, composition,
    backgroundConfig,
    webcamConfig,
    isCropping,
    isCanvasResizeDragging,
    isTimelineOnly: isTimelineOnlyProject,
    setSegment: rawSetSegment,
  });

  const retireSubtitleForHistoryRef = useRef<() => void>(() => {});
  const {
    undo,
    redo,
    canUndo,
    canRedo,
    isBatching,
    beginBatch, bindVisualTransition,
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
    onHistoryRestore: () => retireSubtitleForHistoryRef.current(),
    backgroundConfig,
    backgroundConfigRef,
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
    isProjectTransitionRef,
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
    webcamConfigRef,
  });

  applyUploadedBackgroundRef.current = (url) => setBackgroundConfig((previous) => ({
    ...previous,
    backgroundType: "custom",
    customBackground: url,
  }));

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
    onProjectLoaded: (project) => { onProjectLoadedRef.current(project); return importedProjectOwner(currentProjectIdRef, project.id); },
    onProjectLoadStart: (projectId) => { assertAudioImportWriteAllowed(currentProjectIdRef.current ?? '', true); assertAudioImportWriteAllowed(projectId, true); return beginNarrationProjectLoad(currentProjectIdRef); },
    currentVideo,
    currentAudio,
    currentMicAudio,
    currentWebcamVideo,
  });
  const {
    loadedClipId,
    loadedClipIdRef,
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
    endProjectInteractionShield,
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
    applyLoadedBackgroundConfig, audioFilePath, audioRef, bindSubtitleClipOwner, bindVisualTransition, backgroundConfig, backgroundMutationMetaRef,
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
  uploadTargetResolverRef.current = () => resolveBackgroundUploadTarget({
    project: currentProjectDataRef.current, loadedClipId,
    isTransitioning: isProjectTransitionRef.current || isSwitchingCompositionClipRef.current,
    background: backgroundConfigRef.current, selectionRevision: backgroundSelectionRevisionRef.current,
    clipRevision: clipLoadRequestSeqRef.current,
  });
  const startExportForHarness = useCallback(
    () => exportHook.startExport(),
    [exportHook.startExport],
  );
  const historyProjectResetRef = useAppProjectHarness({
    composition, currentProjectData, currentProjectDataRef, currentProjectIdRef,
    currentRawMicAudioPath, currentRawVideoPath, currentRawWebcamVideoPath, currentRecordingMode,
    duration, editorHistory, handleProjectRawVideoPathChange, projects, rawSetComposition,
    rawSetCurrentRawMicAudioPath, rawSetCurrentRawWebcamVideoPath, rawSetSegment, rawSetWebcamConfig,
    segment, segmentRef, setBackgroundConfigState, setCurrentAudio, setCurrentMicAudio,
    setCurrentProjectData, setCurrentTime, setCurrentVideo, setCurrentWebcamVideo,
    setMousePositions, setPreviewDuration, setThumbnails, startExport: startExportForHarness,
  });
  const [pendingAutoSubtitleProjectId, setPendingAutoSubtitleProjectId] = useState<import('@/hooks/useAppDropActions').PendingAutoSubtitleRequest | null>(null);
  const [pendingAutoSubtitleArmed, setPendingAutoSubtitleArmed] = useState(false);

  const {
    editingKeyframeId,
    setEditingKeyframeId,
    zoomFactor,
    setZoomFactor,
    handleDeleteKeyframe,
    updateZoom,
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
    selectedTextIdsRef, subtitleEditor,
    selectedSubtitleIdsRef,
    handleSelectedTextIdsChange,
    handleSelectedSubtitleIdsChange,
    handleOverlayDragMove,
  } = useAppOverlaySelection({
    segmentRef, segment, composition, currentProjectDataRef, currentProjectIdRef, loadedClipIdRef, isProjectTransitionRef,
    setSegment, setSegmentSilently, setComposition, setCompositionSilently, beginBatch, commitBatch,
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
    retireSubtitleForHistory,
    handleDeleteSubtitle,
  } = useAppSubtitleController({
    subtitleOptions: {
      t: settings.t,
      projectResetKey: currentProjectData?.id ?? null,
      segment,
      segmentRef,
      currentProjectDataRef,
      currentProjectIdRef,
      currentRawVideoPathRef,
      currentRawMicAudioPathRef,
      setSegment,
      setSegmentSilently,
      composition,
      setComposition,
      setCompositionSilently,
      activeClipId,
      loadedClipIdRef,
      currentRawVideoPath,
      currentRawMicAudioPath,
      duration,
      setActivePanel,
      persistProject: (opts) => persistRef.current?.(opts) ?? Promise.resolve(),
    },
    segment: subtitleEditor.segment,
    setSegment: subtitleEditor.setSegment,
    beginBatch,
    commitBatch,
  });
  retireSubtitleForHistoryRef.current = retireSubtitleForHistory;
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
    handleStartRecording,
  } = useAppProjectLifecycleController({
    abortEditorInteractions, applyLoadedBackgroundConfig, armProjectInteractionShieldRelease,
    backgroundConfig, beginProjectInteractionShield, canvasRef, clearClipMediaCaches,
    clipExportSourcePathCacheRef, clipExportWebcamPathCacheRef, clipLoadRequestSeqRef,
    composition, currentAudio, currentMicAudio, currentProjectData, currentProjectDataRef, currentProjectIdRef,
    currentProjectId: projects.currentProjectId, currentRawMicAudioPath, currentRawVideoPath,
    currentRawWebcamVideoPath, currentRecordingMode, currentTime, currentVideo, currentWebcamVideo,
    duration, exportHook, flashRawSavedButton, generateThumbnail, getMonitors,
    handleProjectRawVideoPathChange, handleStopRecording, isPlaying, isProjectInteractionShieldVisible,
    isProjectTransitionRef, isRecording, isSwitchingCompositionClipRef, loadClipAssets,
    loadClipMediaIntoEditor, loadedClipId, monitors, mousePositions, onProjectLoadedRef,
    persistRef, previewContainerRef, projectInteractionBlockCleanupRef, projectInteractionShieldReleaseRef,
    projects, projectsPreviewTargetSnapshotRef, rawAutoCopyEnabled, rawSaveDir,
    rawSetCurrentRawMicAudioPath, rawSetCurrentRawWebcamVideoPath, rawSetCurrentRecordingMode,
    restoreImageRef, seek, segment, setComposition, setCompositionSilently, setCurrentAudio,
    setCurrentMicAudio, setCurrentProjectData, setCurrentTime, setCurrentVideo, setCurrentWebcamVideo, setError,
    setIsProjectInteractionShieldVisible, setIsRawActionBusy, setLastCaptureFps, setLastRawSavedPath,
    setLoadedClipId, setMousePositions, setPreviewAudioResetKey, setPreviewDuration,
    setRawButtonSavedFlash, setShowRawVideoDialog, setSpreadFromClipId,
    setWebcamConfig, spreadAnimationTimerRef, startNewRecording, videoControllerRef,
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
    handleGenerateSubtitles, isGeneratingSubtitles, isPlaceholderBackedProject, isPlaying, loadedClipIdRef,
    loadProjects: projects.loadProjects, pendingAutoSubtitleArmed, pendingAutoSubtitleProjectId,
    projects, rawSetComposition, rawSetSegment, segment, segmentRef, selectedSubtitleIdsRef,
    selectedTextIdsRef, setActivePanel, setComposition, setCompositionSilently, setCurrentProjectData, setError,
    setCurrentVideo, setEditorPreviewDuration, setEditingSubtitleId, setPendingAutoSubtitleArmed,
    setPendingAutoSubtitleProjectId, setSegment, setSubtitleSource, subtitleSource, videoControllerRef,
    videoRef,
  });

  const { handleClearProjects, handleCloseProject, handleDeleteProject, handleRenameProject } = useAppLateEffects({
    backgroundConfig, beginBatch, beginProjectInteractionShield, canRedo, canUndo, canvasRef, commitBatch, composition,
    currentAudio, currentMicAudio, currentProjectDataRef, currentProjectId: projects.currentProjectId, currentProjectIdRef,
    currentRawMicAudioPath, currentRawVideoPath, currentRawWebcamVideoPath, currentRecordingMode,
    currentTime, currentVideo, currentWebcamVideo, duration, editingKeystrokeSegmentId,
    editingKeyframeId, editingPointerId, editingSubtitleId, editingTextId, editorHistory, endProjectInteractionShield,
    exportHook, getKeystrokeTimelineDuration, handleDeleteKeystrokeSegment,
    handleDeletePointerSegment, handleDeleteSubtitle, handleDeleteText, handleOverlayDragMove,
    handleStartRecording, handleTogglePlayPause, historyProjectResetRef, isCropping,
    isDraggingKeystrokeOverlayRef, isRecording, isResizingKeystrokeOverlayRef, mousePositions,
    onStopRecording, persistRef, projects, rawSetComposition, rawSetSegment, redo, seek, segment,
    segmentRef, selectedSubtitleIdsRef, selectedTextIdsRef, setActivePanel, setCurrentAudio,
    setCurrentMicAudio, setCurrentProjectData, setCurrentTime, setCurrentVideo,
    setCurrentWebcamVideo, setEditingKeyframeId, setEditingSubtitleId, setEditingTextId,
    setIsKeystrokeOverlaySelected, setIsKeystrokeResizeDragging, setIsKeystrokeResizeHandleHover,
    setIsPreviewDragging, setLoadedClipId, setMousePositions, setPreviewDuration, setError,
    setSeekIndicatorDir, setSeekIndicatorKey, setSegment, setThumbnails, showHotkeyDialog,
    showRawVideoDialog, tempCanvasRef, undo, videoRef, webcamConfig,
  });

  return (
    <SettingsContext.Provider value={settings}>
      <UserNotificationRegion />
      <AppView {...{
        getNarrationSourceSnapshot: () => readSubtitleSourceSnapshot(currentProjectDataRef, subtitleEditor.segmentRef, loadedClipIdRef, currentRawVideoPathRef, currentRawMicAudioPathRef, segmentRef), renderedProjectEpoch: narrationProjectEpoch(currentProjectIdRef),
        activeClipId, loadedClipId, activePanel, applyNarrationAudioSegments, armProjectInteractionShieldRelease, audioDownloadHook, audioRef, autoSplitMaxUnits,
        autoSplitSubtitles, autoZoomConfig, backgroundConfig, beginBatch, beginProjectInteractionShield, canUseSelectedSubtitleMethod, canvasRef,
        captureFps, captureSource, closeHotkeyDialog, commitBatch, composition, currentProjectData, currentProjectIdRef, currentRawMicAudioPath, currentRawVideoPath,
        currentRecordingMode, currentTime, currentVideo, customCanvasBaseDimensions, duration, editingKeystrokeSegmentId, editingKeyframeId,
        editingSubtitleId, editingTextId, error, exportHook, finalizeNarrationAudioSegments, flushSeek, getAutoCanvasSelectionConfig,
        handleActivateCustomCanvas, handleAddKeystrokeSegment, handleAddPointerSegment, handleAddText, handleApplyCanvasRatioPreset,
        handleApplyCrop, handleAutoZoom, handleAutoZoomConfigChange, handleBackgroundUpload, handleCancelCrop, handleCancelSubtitleGeneration,
        handleClearProjects, handleCloseProject, handleCommitAudioSegments, handleCommitNarrationSegments, handleDeleteAudioSegments, handleDeleteKeyframe,
        handleDeleteNarrationSegments, handleDeleteProject, handleGenerateSubtitles, handleKeystrokeDelayChange, handleLoadProjectFromGrid, handleOpenInsertProjectPicker,
        handleOpenRawVideoDialog, handlePickProjectForSequence, handlePreviewMouseDown, handleRemoveHotkey, handleRemoveRecentUpload,
        handleRemoveSequenceClip, handleRenameProject, handleRequestRecordingAudioAppSelection, handleSelectAllRecordingDeviceAudio, handleSelectMonitorCapture,
        handleSelectSequenceClip, handleSelectWindowCapture, handleSelectedSubtitleIdsChange,
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
        recentUploads, recordingAudioSelection, recordingDuration, restoreImageRef, seek, seekIndicatorDir, seekIndicatorKey, segment: subtitleEditor.segment,
        selectedRecordingMode, selectedSubtitleMethodReason, setActivePanel, setAutoSplitMaxUnits, setAutoSplitSubtitles, setBackgroundConfig,
        setComposition, setCurrentTime, setEditingKeystrokeSegmentId, setEditingKeyframeId, setEditingPointerId, setEditingSubtitleId,
        setEditingTextId, setIsCanvasResizeDragging, setLastRawSavedPath, setProjectPickerMode, setSegment: subtitleEditor.setSegment, setSegmentSilently: subtitleEditor.setSegmentSilently, onPopulateS2sSubtitles: subtitleEditor.populateS2sSubtitles,
        setSelectedRecordingMode, setShowRawVideoDialog, setSubtitleGeminiPrompt, setSubtitleGroqVocabulary,
        setSubtitleLanguageHint, setSubtitleMethod, setSubtitleSource, setTimelineCanvasWidthPx, setWebcamConfig, setZoomFactor, settings,
        showHotkeyDialog, showRawVideoDialog, spreadFromClipId, subtitleGeminiPrompt, subtitleGenerationIndicator,
        subtitleGroqVocabulary, subtitleLanguageHint, subtitleMethod, subtitleMethodCapabilities, subtitleSource, subtitleStatusMessage,
        tempCanvasRef, updateZoom, thumbnails, timelineRef, updatePlaceholderProjectDuration, videoRef, webcamConfig, webcamVideoRef,
        zoomFactor,
      }} />
    </SettingsContext.Provider>
  );
}

export default App;
