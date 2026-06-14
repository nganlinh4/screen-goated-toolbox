import type { MutableRefObject, RefObject } from "react";
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
