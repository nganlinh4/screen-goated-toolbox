import { useState, useRef } from "react";
import "./App.css";
import {
  BackgroundConfig, Project, ProjectComposition,
  VideoSegment, RecordingMode, WebcamConfig,
} from "@/types/video";

import { useUndoRedo } from "@/hooks/useUndoRedo";
import { useHotkeys, useMonitors, useWindows } from "@/hooks/useAppHooks";
import { useProjects, useExport } from "@/hooks/useVideoState";
import { useMediaEngine } from "@/hooks/useMediaEngine";
import { getInitialBackgroundConfig } from "@/lib/appUtils";
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
import { EditorMain } from "@/components/EditorMain";
import { cloneBackgroundConfig } from "@/lib/backgroundConfig";
import { cloneWebcamConfig, DEFAULT_WEBCAM_CONFIG } from "@/lib/webcam";

function App() {
  const settings = useSettingsProvider();
  const {
    state: segment,
    setState: setSegment,
    undo,
    redo,
    canUndo,
    canRedo,
    isBatching,
    beginBatch,
    commitBatch,
  } = useUndoRedo<VideoSegment | null>(null);
  const [activePanel, setActivePanel] = useState<ActivePanel>("background");
  const [isCropping, setIsCropping] = useState(false);
  const [backgroundConfig, setBackgroundConfigState] =
    useState<BackgroundConfig>(() => cloneBackgroundConfig(getInitialBackgroundConfig()));
  const [webcamConfig, setWebcamConfig] = useState<WebcamConfig>(() => cloneWebcamConfig(DEFAULT_WEBCAM_CONFIG));
  const [currentRecordingMode, setCurrentRecordingMode] = useState<RecordingMode>("withoutCursor");
  const [currentProjectData, setCurrentProjectData] = useState<Project | null>(null);
  const [composition, setComposition] = useState<ProjectComposition | null>(null);
  const {
    currentRawVideoPath,
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
  const [currentRawMicAudioPath, setCurrentRawMicAudioPath] = useState("");
  const [currentRawWebcamVideoPath, setCurrentRawWebcamVideoPath] = useState("");
  const [timelineCanvasWidthPx, setTimelineCanvasWidthPx] = useState(0);

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
  // Stable ref for persist callback — avoids cascading useEffect re-triggers
  const persistRef = useRef<((opts?: PersistOptions) => Promise<void>) | null>(null);
  // Early ref so setBackgroundConfig can guard against mid-transition mutations
  const isProjectTransitionRef = useRef(false);

  const {
    backgroundMutationMetaRef,
    setBackgroundConfig,
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
    setSegment,
  });

  const projects = useProjects({
    videoControllerRef,
    setCurrentVideo,
    setCurrentAudio,
    setCurrentMicAudio,
    setCurrentWebcamVideo,
    setSegment,
    setBackgroundConfig,
    setWebcamConfig,
    applyLoadedBackgroundConfig,
    setMousePositions,
    setThumbnails,
    setCurrentRecordingMode,
    setCurrentRawVideoPath: handleProjectRawVideoPathChange,
    onProjectLoaded: (project) => onProjectLoadedRef.current(project),
    currentVideo,
    currentAudio,
    currentMicAudio,
    currentWebcamVideo,
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
    handleTogglePlayPause,
    handleOpenInsertProjectPicker,
    handlePickProjectForSequence,
    handleSelectSequenceClip,
    handleRemoveSequenceClip,
    handleSequenceModeChange,
  } = useCompositionPipeline({
    composition,
    setComposition,
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
    setWebcamConfig,
    setMousePositions,
    setCurrentRecordingMode,
    handleProjectRawVideoPathChange,
    setCurrentRawMicAudioPath,
    setCurrentRawWebcamVideoPath,
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

  // FPS of the most-recent recording (set on stop, cleared when a different project loads).
  const [lastCaptureFps, setLastCaptureFps] = useState<number | null>(null);

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
    currentProjectId: projects.currentProjectId,
    resolveClipExportSourcePath,
    resolveClipExportMicAudioPath,
    resolveClipExportWebcamPath,
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
    handleTextDragMove,
    editingPointerId,
    setEditingPointerId,
    handleSmartPointerHiding,
    handleAddPointerSegment,
    handleDeletePointerSegment,
    handleAutoZoom,
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
    setCurrentRecordingMode,
    setCurrentRawVideoPath: handleProjectRawVideoPathChange,
    setCurrentRawMicAudioPath,
    setCurrentRawWebcamVideoPath,
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
    editingKeystrokeSegmentId,
    editingPointerId,
    setEditingKeyframeId,
    handleDeleteText,
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
    setActivePanel,
    handleTextDragMove,
    beginBatch,
    commitBatch,
  });

  return (
    <SettingsContext.Provider value={settings}>
      <div className="app-container min-h-screen bg-[var(--surface)]">
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
          isPlaying={isPlaying}
          isProcessing={exportHook.isProcessing}
          isVideoReady={isVideoReady}
          hasAppliedCrop={hasAppliedCrop}
          currentTime={currentTime}
          duration={duration}
          handleTogglePlayPause={handleTogglePlayPause}
          handleToggleCrop={handleToggleCrop}
          customCanvasBaseDimensions={customCanvasBaseDimensions}
          getAutoCanvasSelectionConfig={getAutoCanvasSelectionConfig}
          handleActivateCustomCanvas={handleActivateCustomCanvas}
          handleApplyCanvasRatioPreset={handleApplyCanvasRatioPreset}
          segment={segment}
          setSegment={setSegment}
          handleToggleKeystrokeMode={handleToggleKeystrokeMode}
          handleKeystrokeDelayChange={handleKeystrokeDelayChange}
          mousePositionsLength={mousePositions.length}
          handleAutoZoom={handleAutoZoom}
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
          thumbnails={thumbnails}
          timelineRef={timelineRef}
          editingKeystrokeSegmentId={editingKeystrokeSegmentId}
          setCurrentTime={setCurrentTime}
          setEditingKeyframeId={setEditingKeyframeId}
          setEditingTextId={setEditingTextId}
          setEditingKeystrokeSegmentId={setEditingKeystrokeSegmentId}
          setEditingPointerId={setEditingPointerId}
          seek={seek}
          flushSeek={flushSeek}
          handleAddText={handleAddText}
          handleAddKeystrokeSegment={handleAddKeystrokeSegment}
          handleAddPointerSegment={handleAddPointerSegment}
          setTimelineCanvasWidthPx={setTimelineCanvasWidthPx}
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
          isProjectInteractionShieldVisible={isProjectInteractionShieldVisible}
          isCropping={isCropping}
          currentVideo={currentVideo}
          segment={segment}
          currentTime={currentTime}
          onCancelCrop={handleCancelCrop}
          onApplyCrop={handleApplyCrop}
          exportHook={exportHook}
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
