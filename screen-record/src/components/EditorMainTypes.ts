import type {
  ChangeEvent,
  Dispatch,
  MouseEvent,
  MutableRefObject,
  RefObject,
  SetStateAction,
} from "react";
import type { CanvasModeToggleProps } from "@/components/CanvasModeToggle";
import type { KeystrokeEditFrame } from "@/components/PreviewCanvas";
import type { ActivePanel } from "@/components/sidepanel/index";
import type { SubtitleMethod } from "@/hooks/useSubtitleGeneration";
import type { SubtitleSource } from "@/lib/subtitleGenerationPlan";
import type { SubtitleGenerationIndicator } from "@/lib/subtitleGenerationPlan";
import type { TrackSelectionRange } from "@/lib/timelineSegmentSelection";
import type {
  AudioDownloadTrackKind,
  AudioGainPoint,
  AutoZoomConfig,
  BackgroundConfig,
  ImportedAudioSegment,
  NarrationSegment,
  ProjectComposition,
  VideoSegment,
  WebcamConfig,
} from "@/types/video";

export interface EditorMainProps {
  error: string | null;
  isOverlayMode: boolean;
  previewContainerRef: MutableRefObject<HTMLDivElement | null>;
  previewCursorClass: string;
  handlePreviewMouseDown: (e: MouseEvent<HTMLDivElement>) => void;
  canvasRef: RefObject<HTMLCanvasElement | null>;
  tempCanvasRef: RefObject<HTMLCanvasElement | null>;
  videoRef: RefObject<HTMLVideoElement | null>;
  webcamVideoRef: RefObject<HTMLVideoElement | null>;
  audioRef: RefObject<HTMLAudioElement | null>;
  micAudioRef: RefObject<HTMLAudioElement | null>;
  previousPreloadVideoRef: RefObject<HTMLVideoElement | null>;
  previousPreloadAudioRef: RefObject<HTMLAudioElement | null>;
  nextPreloadVideoRef: RefObject<HTMLVideoElement | null>;
  nextPreloadAudioRef: RefObject<HTMLAudioElement | null>;
  keystrokeOverlayEditFrame: KeystrokeEditFrame | null;
  isKeystrokeOverlaySelected: boolean;
  isDraggingKeystrokeOverlayRef: MutableRefObject<boolean>;
  isResizingKeystrokeOverlayRef: MutableRefObject<boolean>;
  isBuffering: boolean;
  isPreviewPlaying: boolean;
  currentVideo: string | null;
  isTimelineOnly: boolean;
  isLoadingVideo: boolean;
  loadingProgress: number;
  isRecording: boolean;
  recordingDuration: number;
  isCropping: boolean;
  backgroundConfig: BackgroundConfig;
  setBackgroundConfig: (
    update: BackgroundConfig | ((prev: BackgroundConfig) => BackgroundConfig),
  ) => void;
  beginBatch: () => void;
  commitBatch: () => void;
  setIsCanvasResizeDragging: (dragging: boolean) => void;
  seekIndicatorDir: "left" | "right" | null;
  seekIndicatorKey: number;
  audioResetKey?: number;
  isPlaying: boolean;
  isProcessing: boolean;
  isVideoReady: boolean;
  hasAppliedCrop: boolean;
  currentTime: number;
  duration: number;
  handleTogglePlayPause: () => void;
  handleToggleCrop: () => void;
  onSetProjectDuration?: (duration: number) => void;
  customCanvasBaseDimensions: { width: number; height: number };
  getAutoCanvasSelectionConfig: CanvasModeToggleProps["getAutoCanvasSelectionConfig"];
  handleActivateCustomCanvas: () => void;
  handleApplyCanvasRatioPreset: (ratioWidth: number, ratioHeight: number) => void;
  isAutoCanvasDisabled?: boolean;
  segment: VideoSegment | null;
  setSegment: (s: VideoSegment | null) => void;
  setSegmentSilently?: (s: VideoSegment | null) => void;
  composition: ProjectComposition | null;
  setComposition: (
    composition:
      | ProjectComposition
      | null
      | ((prev: ProjectComposition | null) => ProjectComposition | null),
  ) => void;
  handleToggleKeystrokeMode: () => void;
  handleKeystrokeDelayChange: (delay: number) => void;
  mousePositionsLength: number;
  handleAutoZoom: () => void;
  autoZoomConfig: AutoZoomConfig;
  handleAutoZoomConfigChange: (config: AutoZoomConfig) => void;
  handleSmartPointerHiding: () => void;
  activePanel: ActivePanel;
  setActivePanel: (panel: ActivePanel) => void;
  editingKeyframeId: number | null;
  zoomFactor: number;
  setZoomFactor: (factor: number) => void;
  handleDeleteKeyframe: () => void;
  throttledUpdateZoom: (updates: { zoomFactor?: number; positionX?: number; positionY?: number }) => void;
  webcamConfig: WebcamConfig;
  setWebcamConfig: Dispatch<SetStateAction<WebcamConfig>>;
  recentUploads: string[];
  handleRemoveRecentUpload: (url: string) => void;
  handleBackgroundUpload: (e: ChangeEvent<HTMLInputElement>) => void;
  isBackgroundUploadProcessing: boolean;
  editingTextId: string | null;
  editingSubtitleId: string | null;
  subtitleSource: SubtitleSource;
  onSubtitleSourceChange: (value: SubtitleSource) => void;
  subtitleMethod: SubtitleMethod;
  onSubtitleMethodChange: (value: SubtitleMethod) => void;
  subtitleMethodCapabilities: Array<{ method: SubtitleMethod; available: boolean; reason?: string | null }>;
  canUseSelectedSubtitleMethod: boolean;
  selectedSubtitleMethodReason?: string | null;
  subtitleLanguageHint: string;
  onSubtitleLanguageHintChange: (value: string) => void;
  subtitleGeminiPrompt: string;
  onSubtitleGeminiPromptChange: (value: string) => void;
  subtitleGroqVocabulary: string[];
  onSubtitleGroqVocabularyChange: (value: string[]) => void;
  autoSplitSubtitles: boolean;
  onAutoSplitSubtitlesChange: (value: boolean) => void;
  autoSplitSubtitleMaxUnits: number;
  onAutoSplitSubtitleMaxUnitsChange: (value: number) => void;
  isGeneratingSubtitles: boolean;
  subtitleStatusMessage?: string | null;
  subtitleGenerationIndicator?: SubtitleGenerationIndicator | null;
  handleGenerateSubtitles: (selectedRange?: TrackSelectionRange | null) => void;
  handleCancelSubtitleGeneration: () => void;
  onApplyNarrationSegments: (
    segments: NarrationSegment[],
    replaceSubtitleIds: string[],
  ) => void | Promise<void>;
  onFinalizeNarrationSegments: () => void | Promise<void>;
  onSelectedTextIdsChange?: (ids: string[]) => void;
  onSelectedSubtitleIdsChange?: (ids: string[]) => void;
  projectResetKey?: string | null;
  currentRawVideoPath: string;
  currentRawMicAudioPath: string;
  currentProjectName?: string | null;
  thumbnails: string[];
  timelineRef: RefObject<HTMLDivElement | null>;
  editingKeystrokeSegmentId: string | null;
  setCurrentTime: (time: number) => void;
  setEditingKeyframeId: (id: number | null) => void;
  setEditingTextId: (id: string | null) => void;
  setEditingSubtitleId: (id: string | null) => void;
  setEditingKeystrokeSegmentId: (id: string | null) => void;
  setEditingPointerId: (id: string | null) => void;
  seek: (time: number) => void;
  flushSeek: () => void;
  handleAddText: () => void;
  handleAddKeystrokeSegment: (atTime?: number) => void;
  handleAddPointerSegment: (atTime?: number) => void;
  setTimelineCanvasWidthPx: (width: number) => void;
  onPickImportedAudioFile?: (file: File) => void;
  onUpdateAudioSegment?: (
    id: string,
    patch: Partial<ImportedAudioSegment>,
  ) => void;
  onDeleteAudioSegments?: (ids: string[]) => void;
  onCommitAudioSegments?: () => void;
  audioTrackVolumePoints?: AudioGainPoint[];
  onUpdateAudioTrackVolumePoints?: (points: AudioGainPoint[]) => void;
  narrationSegments?: NarrationSegment[];
  onUpdateNarrationSegment?: (
    id: string,
    patch: Partial<NarrationSegment>,
  ) => void;
  onDeleteNarrationSegments?: (ids: string[]) => void;
  onCommitNarrationSegments?: () => void;
  narrationTrackVolumePoints?: AudioGainPoint[];
  onUpdateNarrationTrackVolumePoints?: (points: AudioGainPoint[]) => void;
  onAudioTrackDownload?: (
    trackKind: AudioDownloadTrackKind,
    trackLabel: string,
  ) => void;
}
