import type { MutableRefObject, RefObject } from "react";
import { Header } from "@/components/Header";
import { SequencePillChain } from "@/components/SequencePillChain";
import { DragDropOverlay } from "@/components/DragDropOverlay";
import { EditorMain } from "@/components/EditorMain";
import { EditorOverlays } from "@/components/EditorOverlays";
import { ResizeBorders } from "@/components/layout/ResizeBorders";
import type { ActivePanel } from "@/components/sidepanel/index";
import type { KeystrokeEditFrame } from "@/components/PreviewCanvas";
import type { ProjectsPreviewTargetSnapshot } from "@/components/ProjectsView";
import type { CanvasModeToggleProps } from "@/components/CanvasModeToggle";
import type { Hotkey, MonitorInfo, WindowInfo } from "@/hooks/useAppHooks";
import type { useSettingsProvider } from "@/hooks/useSettings";
import type { SubtitleMethod } from "@/hooks/useSubtitleGeneration";
import type {
  SubtitleSource,
  SubtitleGenerationIndicator,
} from "@/lib/subtitleGenerationPlan";
import type { TrackSelectionRange } from "@/lib/timelineSegmentSelection";
import type { RecordingAudioSelection } from "@/types/recordingAudio";
import type {
  AudioGainPoint,
  AutoZoomConfig,
  BackgroundConfig,
  ImportedAudioSegment,
  MousePosition,
  NarrationSegment,
  Project,
  ProjectComposition,
  ProjectCompositionMode,
  RecordingMode,
  VideoSegment,
  WebcamConfig,
} from "@/types/video";
import type {
  AudioDownloadHook,
  BackgroundConfigSetter,
  CompositionSetter,
  Dispatch,
  ExportHook,
  ProjectsState,
  SegmentSetter,
  SetStateAction,
} from "@/hooks/appControllerTypes";

type SettingsState = ReturnType<typeof useSettingsProvider>;

export interface AppViewProps {
  // Identity / panels
  activeClipId: string | null;
  activePanel: ActivePanel;
  setActivePanel: (panel: ActivePanel) => void;

  // Settings
  settings: SettingsState;

  // Project / composition state
  composition: ProjectComposition | null;
  setComposition: CompositionSetter;
  currentProjectData: Project | null;
  projects: ProjectsState;
  projectPickerMode: "insertBefore" | "insertAfter" | null;
  setProjectPickerMode: (mode: "insertBefore" | "insertAfter" | null) => void;
  spreadFromClipId: string | null;

  // Segment
  segment: VideoSegment | null;
  setSegment: SegmentSetter;
  setSegmentSilently?: (s: VideoSegment | null) => void;

  // Background / webcam
  backgroundConfig: BackgroundConfig;
  setBackgroundConfig: BackgroundConfigSetter;
  webcamConfig: WebcamConfig;
  setWebcamConfig: Dispatch<SetStateAction<WebcamConfig>>;

  // Media refs
  audioRef: RefObject<HTMLAudioElement | null>;
  micAudioRef: RefObject<HTMLAudioElement | null>;
  videoRef: RefObject<HTMLVideoElement | null>;
  webcamVideoRef: RefObject<HTMLVideoElement | null>;
  canvasRef: RefObject<HTMLCanvasElement | null>;
  tempCanvasRef: RefObject<HTMLCanvasElement | null>;
  previousPreloadVideoRef: RefObject<HTMLVideoElement | null>;
  previousPreloadAudioRef: RefObject<HTMLAudioElement | null>;
  nextPreloadVideoRef: RefObject<HTMLVideoElement | null>;
  nextPreloadAudioRef: RefObject<HTMLAudioElement | null>;
  previewContainerRef: MutableRefObject<HTMLDivElement | null>;
  timelineRef: RefObject<HTMLDivElement>;
  restoreImageRef: MutableRefObject<string | null>;
  projectsPreviewTargetSnapshotRef: MutableRefObject<ProjectsPreviewTargetSnapshot | null>;
  isDraggingKeystrokeOverlayRef: MutableRefObject<boolean>;
  isResizingKeystrokeOverlayRef: MutableRefObject<boolean>;

  // Playback / timeline scalar state
  currentTime: number;
  setCurrentTime: (time: number) => void;
  duration: number;
  isPlaying: boolean;
  isBuffering: boolean;
  isVideoReady: boolean;
  isLoadingVideo: boolean;
  loadingProgress: number;
  currentVideo: string | null;
  thumbnails: string[];
  mousePositions: MousePosition[];
  seek: (time: number) => void;
  flushSeek: () => void;
  handleTogglePlayPause: () => void;
  previewAudioResetKey: number;
  seekIndicatorDir: "left" | "right" | null;
  seekIndicatorKey: number;
  setTimelineCanvasWidthPx: (width: number) => void;

  // Recording
  isRecording: boolean;
  recordingDuration: number;
  currentRecordingMode: RecordingMode;
  selectedRecordingMode: RecordingMode;
  setSelectedRecordingMode: (mode: RecordingMode) => void;
  captureSource: "monitor" | "window";
  captureFps: number | null;
  monitors: MonitorInfo[];
  windows: WindowInfo[];
  recordingAudioSelection: RecordingAudioSelection;
  isSelectingRecordingAudioApp: boolean;
  handleToggleRecordingDeviceAudio: (enabled: boolean) => void;
  handleToggleRecordingMicAudio: (enabled: boolean) => void;
  handleSelectAllRecordingDeviceAudio: () => void;
  handleRequestRecordingAudioAppSelection: () => void;
  handleSelectMonitorCapture: (monitorId: string, fps: number | null) => void;
  handleSelectWindowCapture: (fps: number | null) => void;
  handleSelectWindowForRecording: (
    windowId: string,
    captureMethod: "game" | "window",
  ) => void;
  showWindowSelect: boolean;
  setShowWindowSelect: (show: boolean) => void;

  // Raw video
  currentRawVideoPath: string;
  currentRawMicAudioPath: string;
  lastRawSavedPath: string;
  setLastRawSavedPath: (path: string) => void;
  rawAutoCopyEnabled: boolean;
  isRawActionBusy: boolean;
  rawButtonSavedFlash: boolean;
  showRawVideoDialog: boolean;
  setShowRawVideoDialog: (show: boolean) => void;
  handleOpenRawVideoDialog: () => void;
  handleToggleRawAutoCopy: (enabled: boolean) => void;

  // Hotkeys
  hotkeys: Hotkey[];
  handleRemoveHotkey: (index: number) => void;
  openHotkeyDialog: () => void;
  closeHotkeyDialog: () => void;
  showHotkeyDialog: boolean;

  // Errors / overlay mode
  error: string | null;
  isOverlayMode: boolean;
  isPlaceholderBackedProject: boolean;
  isTimelineOnlyProject: boolean;

  // Export / audio download hooks
  exportHook: ExportHook;
  audioDownloadHook: AudioDownloadHook;

  // History batching
  beginBatch: () => void;
  commitBatch: () => void;

  // Canvas / crop
  customCanvasBaseDimensions: { width: number; height: number };
  getAutoCanvasSelectionConfig: CanvasModeToggleProps["getAutoCanvasSelectionConfig"];
  handleActivateCustomCanvas: () => void;
  handleApplyCanvasRatioPreset: (ratioWidth: number, ratioHeight: number) => void;
  setIsCanvasResizeDragging: (dragging: boolean) => void;
  isCropping: boolean;
  hasAppliedCrop: boolean;
  handleToggleCrop: () => void;
  handleCancelCrop: () => void;
  handleApplyCrop: (crop: VideoSegment["crop"]) => void;
  updatePlaceholderProjectDuration: (
    nextDuration: number,
    reason: string,
  ) => void | Promise<void>;

  // Zoom / keyframes
  autoZoomConfig: AutoZoomConfig;
  handleAutoZoom: () => void;
  handleAutoZoomConfigChange: (config: AutoZoomConfig) => void;
  handleSmartPointerHiding: () => void;
  editingKeyframeId: number | null;
  setEditingKeyframeId: (id: number | null) => void;
  handleDeleteKeyframe: () => void;
  zoomFactor: number;
  setZoomFactor: (factor: number) => void;
  throttledUpdateZoom: (updates: {
    zoomFactor?: number;
    positionX?: number;
    positionY?: number;
  }) => void;

  // Keystroke overlay
  keystrokeOverlayEditFrame: KeystrokeEditFrame | null;
  isKeystrokeOverlaySelected: boolean;
  editingKeystrokeSegmentId: string | null;
  setEditingKeystrokeSegmentId: (id: string | null) => void;
  handleToggleKeystrokeMode: () => void;
  handleKeystrokeDelayChange: (delay: number) => void;
  handleAddKeystrokeSegment: (atTime?: number) => void;

  // Text / pointer / subtitle editing
  editingTextId: string | null;
  setEditingTextId: (id: string | null) => void;
  handleAddText: () => void;
  setEditingPointerId: (id: string | null) => void;
  handleAddPointerSegment: (atTime?: number) => void;
  editingSubtitleId: string | null;
  setEditingSubtitleId: (id: string | null) => void;
  handleSelectedTextIdsChange: (ids: string[]) => void;
  handleSelectedSubtitleIdsChange: (ids: string[]) => void;
  previewCursorClass: string;
  handlePreviewMouseDown: (e: React.MouseEvent<HTMLDivElement>) => void;

  // Subtitle generation config
  subtitleSource: SubtitleSource;
  setSubtitleSource: (value: SubtitleSource) => void;
  subtitleMethod: SubtitleMethod;
  setSubtitleMethod: (value: SubtitleMethod) => void;
  subtitleMethodCapabilities: Array<{
    method: SubtitleMethod;
    available: boolean;
    reason?: string | null;
  }>;
  canUseSelectedSubtitleMethod: boolean;
  selectedSubtitleMethodReason?: string | null;
  subtitleLanguageHint: string;
  setSubtitleLanguageHint: (value: string) => void;
  subtitleGeminiPrompt: string;
  setSubtitleGeminiPrompt: (value: string) => void;
  subtitleGroqVocabulary: string[];
  setSubtitleGroqVocabulary: (value: string[]) => void;
  autoSplitSubtitles: boolean;
  setAutoSplitSubtitles: (value: boolean) => void;
  autoSplitMaxUnits: number;
  setAutoSplitMaxUnits: (value: number) => void;
  isGeneratingSubtitles: boolean;
  subtitleStatusMessage?: string | null;
  subtitleGenerationIndicator?: SubtitleGenerationIndicator | null;
  handleGenerateSubtitles: (selectedRange?: TrackSelectionRange | null) => void;
  handleCancelSubtitleGeneration: () => void;

  // Narration / audio segments
  applyNarrationAudioSegments: (
    segments: NarrationSegment[],
    replaceSubtitleIds: string[],
  ) => void | Promise<void>;
  finalizeNarrationAudioSegments: () => void | Promise<void>;
  handleCommitAudioSegments?: () => void;
  handleCommitNarrationSegments?: () => void;
  handleDeleteAudioSegments?: (ids: string[]) => void;
  handleDeleteNarrationSegments?: (ids: string[]) => void;
  handleUpdateAudioSegment?: (
    id: string,
    patch: Partial<ImportedAudioSegment>,
  ) => void;
  handleUpdateAudioTrackVolumePoints?: (points: AudioGainPoint[]) => void;
  handleUpdateNarrationSegment?: (
    id: string,
    patch: Partial<NarrationSegment>,
  ) => void;
  handleUpdateNarrationTrackVolumePoints?: (points: AudioGainPoint[]) => void;

  // Imports
  importAudio: (file: File) => void;
  importAudios?: (files: File[]) => void;
  importSubtitleFile: (file: File) => void;
  importVideo: (file: File) => void;
  isImporting: boolean;
  isImportingAudio: boolean;
  isImportingSubtitle: boolean;
  recentUploads: string[];
  handleRemoveRecentUpload: (url: string) => void;
  handleBackgroundUpload: (e: React.ChangeEvent<HTMLInputElement>) => void;
  isBackgroundUploadProcessing: boolean;

  // Project lifecycle
  beginProjectInteractionShield: () => void;
  armProjectInteractionShieldRelease: () => void;
  isProjectInteractionShieldVisible: boolean;
  handleLoadProjectFromGrid: (id: string) => Promise<void>;
  handleToggleProjects: () => void;
  handleCloseProject: () => void;
  handleOpenInsertProjectPicker: (
    clipId: string | null,
    placement: "before" | "after",
  ) => void;
  handlePickProjectForSequence: (id: string) => void;
  handleSelectSequenceClip: (clipId: string) => void | Promise<void>;
  handleRemoveSequenceClip: (clipId: string) => void | Promise<void>;
  handleSequenceModeChange: (mode: ProjectCompositionMode) => void | Promise<void>;
}

export function AppView(props: AppViewProps) {
  const {
    activeClipId,
    activePanel,
    applyNarrationAudioSegments,
    armProjectInteractionShieldRelease,
    audioDownloadHook,
    audioRef,
    autoSplitMaxUnits,
    autoSplitSubtitles,
    autoZoomConfig,
    backgroundConfig,
    beginBatch,
    beginProjectInteractionShield,
    canUseSelectedSubtitleMethod,
    canvasRef,
    captureFps,
    captureSource,
    closeHotkeyDialog,
    commitBatch,
    composition,
    currentProjectData,
    currentRawMicAudioPath,
    currentRawVideoPath,
    currentRecordingMode,
    currentTime,
    currentVideo,
    customCanvasBaseDimensions,
    duration,
    editingKeystrokeSegmentId,
    editingKeyframeId,
    editingSubtitleId,
    editingTextId,
    error,
    exportHook,
    finalizeNarrationAudioSegments,
    flushSeek,
    getAutoCanvasSelectionConfig,
    handleActivateCustomCanvas,
    handleAddKeystrokeSegment,
    handleAddPointerSegment,
    handleAddText,
    handleApplyCanvasRatioPreset,
    handleApplyCrop,
    handleAutoZoom,
    handleAutoZoomConfigChange,
    handleBackgroundUpload,
    handleCancelCrop,
    handleCancelSubtitleGeneration,
    handleCloseProject,
    handleCommitAudioSegments,
    handleCommitNarrationSegments,
    handleDeleteAudioSegments,
    handleDeleteKeyframe,
    handleDeleteNarrationSegments,
    handleGenerateSubtitles,
    handleKeystrokeDelayChange,
    handleLoadProjectFromGrid,
    handleOpenInsertProjectPicker,
    handleOpenRawVideoDialog,
    handlePickProjectForSequence,
    handlePreviewMouseDown,
    handleRemoveHotkey,
    handleRemoveRecentUpload,
    handleRemoveSequenceClip,
    handleSelectMonitorCapture,
    handleSelectSequenceClip,
    handleSelectWindowCapture,
    handleSelectWindowForRecording,
    handleSelectedSubtitleIdsChange,
    handleSelectedTextIdsChange,
    handleSequenceModeChange,
    handleSmartPointerHiding,
    handleToggleCrop,
    handleToggleKeystrokeMode,
    handleTogglePlayPause,
    handleToggleProjects,
    handleToggleRawAutoCopy,
    handleToggleRecordingDeviceAudio,
    handleToggleRecordingMicAudio,
    handleUpdateAudioSegment,
    handleUpdateAudioTrackVolumePoints,
    handleUpdateNarrationSegment,
    handleUpdateNarrationTrackVolumePoints,
    hasAppliedCrop,
    hotkeys,
    importAudio,
    importAudios,
    importSubtitleFile,
    importVideo,
    isBackgroundUploadProcessing,
    isBuffering,
    isCropping,
    isDraggingKeystrokeOverlayRef,
    isGeneratingSubtitles,
    isImporting,
    isImportingAudio,
    isImportingSubtitle,
    isKeystrokeOverlaySelected,
    isLoadingVideo,
    isOverlayMode,
    isPlaceholderBackedProject,
    isPlaying,
    isProjectInteractionShieldVisible,
    isRawActionBusy,
    isRecording,
    isSelectingRecordingAudioApp,
    isResizingKeystrokeOverlayRef,
    isTimelineOnlyProject,
    isVideoReady,
    keystrokeOverlayEditFrame,
    lastRawSavedPath,
    loadingProgress,
    micAudioRef,
    monitors,
    mousePositions,
    nextPreloadAudioRef,
    nextPreloadVideoRef,
    openHotkeyDialog,
    previewAudioResetKey,
    previewContainerRef,
    previewCursorClass,
    previousPreloadAudioRef,
    previousPreloadVideoRef,
    projectPickerMode,
    projects,
    projectsPreviewTargetSnapshotRef,
    rawAutoCopyEnabled,
    rawButtonSavedFlash,
    recentUploads,
    recordingAudioSelection,
    recordingDuration,
    restoreImageRef,
    seek,
    seekIndicatorDir,
    seekIndicatorKey,
    segment,
    selectedRecordingMode,
    setActivePanel,
    setAutoSplitMaxUnits,
    setAutoSplitSubtitles,
    setBackgroundConfig,
    setComposition,
    setCurrentTime,
    setEditingKeystrokeSegmentId,
    setEditingKeyframeId,
    setEditingPointerId,
    setEditingSubtitleId,
    setEditingTextId,
    setIsCanvasResizeDragging,
    setLastRawSavedPath,
    setProjectPickerMode,
    setSegment,
    setSegmentSilently,
    setSelectedRecordingMode,
    setShowRawVideoDialog,
    setShowWindowSelect,
    setSubtitleGeminiPrompt,
    setSubtitleGroqVocabulary,
    setSubtitleLanguageHint,
    setSubtitleMethod,
    setSubtitleSource,
    setTimelineCanvasWidthPx,
    setWebcamConfig,
    setZoomFactor,
    settings,
    showHotkeyDialog,
    showRawVideoDialog,
    showWindowSelect,
    spreadFromClipId,
    subtitleGeminiPrompt,
    subtitleGenerationIndicator,
    subtitleGroqVocabulary,
    subtitleLanguageHint,
    subtitleMethod,
    subtitleMethodCapabilities,
    subtitleSource,
    subtitleStatusMessage,
    tempCanvasRef,
    throttledUpdateZoom,
    thumbnails,
    timelineRef,
    updatePlaceholderProjectDuration,
    videoRef,
    webcamConfig,
    webcamVideoRef,
    windows,
    zoomFactor,
  } = props;

  return (
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
        onSelectAllRecordingDeviceAudio={props.handleSelectAllRecordingDeviceAudio}
        onRequestRecordingAudioAppSelection={props.handleRequestRecordingAudioAppSelection}
        rawButtonLabel={rawButtonSavedFlash ? settings.t.rawVideoSavedButton : settings.t.saveRawVideo}
        rawButtonPulse={currentRecordingMode === "withCursor"}
        rawButtonDisabled={!currentRawVideoPath && !lastRawSavedPath}
        onOpenRawVideoDialog={handleOpenRawVideoDialog}
        onExport={exportHook.handleExport}
        onOpenProjects={handleToggleProjects}
        projectsButtonDisabled={isProjectInteractionShieldVisible}
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
        setSegmentSilently={setSegmentSilently}
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
        selectedSubtitleMethodReason={props.selectedSubtitleMethodReason}
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
        onUpdateAudioSegment={handleUpdateAudioSegment}
        onDeleteAudioSegments={handleDeleteAudioSegments}
        onCommitAudioSegments={handleCommitAudioSegments}
        audioTrackVolumePoints={composition?.audioTrackVolumePoints}
        onUpdateAudioTrackVolumePoints={handleUpdateAudioTrackVolumePoints}
        narrationSegments={composition?.narrationSegments}
        onUpdateNarrationSegment={handleUpdateNarrationSegment}
        onDeleteNarrationSegments={handleDeleteNarrationSegments}
        onCommitNarrationSegments={handleCommitNarrationSegments}
        narrationTrackVolumePoints={composition?.narrationTrackVolumePoints}
        onUpdateNarrationTrackVolumePoints={handleUpdateNarrationTrackVolumePoints}
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
  );
}
