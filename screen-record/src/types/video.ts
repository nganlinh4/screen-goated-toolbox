import type { CursorPack } from "@/lib/renderer/cursorModel";

export type {
  BakedCameraFrame,
  BakedCursorFrame,
  BakedKeystrokeOverlay,
  BakedOverlayPayload,
  BakedTextOverlay,
  BakedWebcamFrame,
  ExportArtifact,
  ExportArtifactFormat,
  ExportOptions,
  OverlayFrame,
  OverlayQuad,
  VideoMetadata,
} from "./videoExportTypes";

// Resolution/FPS options are computed dynamically from canvas dimensions

export interface ZoomKeyframe {
  time: number;
  duration: number;
  zoomFactor: number;
  positionX: number;
  positionY: number;
  easingType: "linear" | "easeOut" | "easeInOut";
}

/**
 * A discrete, bounded zoom region on the timeline (Screen Studio-style).
 * The camera eases in over `easeIn` seconds, holds the target across the body,
 * then eases out over `easeOut` seconds. Outside the block the camera reverts to
 * the auto path (or default), so gaps between blocks naturally show auto-zoom.
 */
export interface ZoomBlock {
  id: string;
  startTime: number; // block begins (camera starts easing in)
  endTime: number; // block ends (camera fully eased back out)
  easeIn: number; // ramp-in duration in seconds
  easeOut: number; // ramp-out duration in seconds
  zoomFactor: number; // hold target
  positionX: number; // 0..1 anchor
  positionY: number; // 0..1 anchor
  followCursor?: boolean; // when true, anchor follows the auto path inside the block
  enabled: boolean; // disable without deleting
}

export interface TextBackground {
  enabled: boolean;
  color: string;
  opacity: number; // 0-1, background pill opacity
  paddingX: number;
  paddingY: number;
  borderRadius: number;
}

export interface TextWrapStyle {
  enabled: boolean;
  maxWidthPercent: number; // percentage of canvas width
}

export interface TextStrokeStyle {
  enabled: boolean;
  color: string;
  width: number;
  opacity: number; // 0-1
}

export interface TextShadowStyle {
  enabled: boolean;
  color: string;
  blur: number;
  offsetX: number;
  offsetY: number;
  opacity: number; // 0-1
}

export type TextAnimationPreset = "none" | "fade" | "slide-up" | "pop";

export interface TextAnimationStyle {
  preset: TextAnimationPreset;
  inDuration: number; // seconds
  outDuration: number; // seconds
}

export interface TextStyle {
  fontSize: number;
  color: string;
  x: number; // 0-100 percentage
  y: number; // 0-100 percentage
  fontWeight?: "normal" | "bold";
  fontVariations?: {
    wght?: number; // 100-900, default 400
    wdth?: number; // 75-125, default 100
    slnt?: number; // -12 to 0, default 0
    ROND?: number; // 0-100, default 0
  };
  textAlign?: "left" | "center" | "right";
  opacity?: number; // 0-1, default 1
  letterSpacing?: number; // px, default 0
  lineHeight?: number; // multiplier, default 1.25
  wrap?: TextWrapStyle;
  stroke?: TextStrokeStyle;
  shadow?: TextShadowStyle;
  animation?: TextAnimationStyle;
  background?: TextBackground;
}

export interface TextSegment {
  id: string;
  startTime: number;
  endTime: number;
  text: string;
  style: TextStyle;
  splitGroupId?: string;
  splitGroupIndex?: number;
  splitGroupCount?: number;
  splitGroupText?: string;
  splitGroupStartTime?: number;
  splitGroupEndTime?: number;
  sourceGroup?: SubtitleSourceGroup;
  provenance?: SubtitleProvenance;
}

export type SubtitleSegment = TextSegment;

export interface SubtitleProvenance {
  sourceKind: "audio";
  audioSegmentId: string;
  sourceName: string;
  sourcePath: string;
  sourceLocalStartTime: number;
  sourceLocalEndTime: number;
}

export type SubtitleSourceGroupKind = "audio" | "video" | "mic" | "unassigned";
export type SubtitleSourceGroupAssignment = "generated" | "manual" | "inferred";

export interface SubtitleSourceGroup {
  kind: SubtitleSourceGroupKind;
  assignment?: SubtitleSourceGroupAssignment;
  audioSegmentId?: string;
  sourceName?: string;
  sourcePath?: string;
}

export type SubtitleTrackKind = "original" | "translation";

export interface SubtitleTrack {
  id: string;
  kind: SubtitleTrackKind;
  slotLabel?: string | null;
  targetLanguage?: string | null;
  segments: SubtitleSegment[];
}

export interface SubtitleViewState {
  kind: "track" | "custom";
  trackId?: string | null;
}

export type SubtitleChainItem =
  | {
      type: "track";
      trackId: string;
    }
  | {
      type: "delimiter";
      value: string;
    };

export interface CursorVisibilitySegment {
  id: string;
  startTime: number;
  endTime: number;
}

export type KeystrokeMode = "off" | "keyboard" | "keyboardMouse";

export interface InputModifiers {
  ctrl?: boolean;
  alt?: boolean;
  shift?: boolean;
  win?: boolean;
}

export interface RawInputEvent {
  type: "keyboard" | "mousedown" | "wheel";
  timestamp: number;
  vk?: number;
  key?: string;
  btn?: "left" | "right" | "middle";
  direction?: "up" | "down" | "none";
  modifiers?: InputModifiers;
}

export interface KeystrokeEvent {
  id: string;
  type: "keyboard" | "mousedown" | "wheel";
  startTime: number;
  endTime: number;
  label: string;
  count: number;
  isHold?: boolean;
  modifiers: InputModifiers;
  key?: string;
  btn?: "left" | "right" | "middle";
  direction?: "up" | "down" | "none";
}

export interface KeystrokeOverlayConfig {
  x: number; // 0-100 (% of canvas width), lane barrier anchor
  y: number; // 0-100 (% of canvas height), baseline anchor
  scale: number; // uniform scale
}

export interface CropRect {
  x: number; // 0-1
  y: number; // 0-1
  width: number; // 0-1
  height: number; // 0-1
}

export interface TrimSegment {
  id: string;
  startTime: number;
  endTime: number;
}

export interface SpeedPoint {
  time: number;
  speed: number;
}

export interface AudioGainPoint {
  time: number;
  volume: number;
}

export type DeviceAudioPoint = AudioGainPoint;
export type MicAudioPoint = AudioGainPoint;
export type ImportedAudioGainPoint = AudioGainPoint;
export type AudioDownloadTrackKind = "device" | "mic" | "imported" | "narration";
export type AudioDownloadFormat = "mp3" | "wav";

export interface AudioDownloadResult {
  status?: string;
  path?: string;
  format?: AudioDownloadFormat;
  bytes?: number;
  trackKind?: AudioDownloadTrackKind;
}

/**
 * An imported audio file placed on the project-level Audio track. Distinct from
 * device audio (screen capture) and mic audio (microphone capture) — these
 * are user-supplied audio files.
 *
 * `startTime` is in **timeline seconds** (project-wide), not per-clip seconds.
 * Reordering clips does NOT auto-shift imported audio — by design.
 */
export interface ImportedAudioSegment {
  id: string;
  rawAudioPath: string;
  name: string;
  duration: number;
  startTime: number;
  inPoint: number;
  outPoint: number;
  /** Per-segment playback rate (1.0 = original; clamp 0.25–4.0). */
  playbackRate?: number;
  addedAt: number;
}

/**
 * A TTS-narration audio clip placed on the project-level Narration track.
 * Lives in `composition.narrationSegments` and is rendered/exported separately
 * from `composition.audioSegments` so the two features never share a row.
 */
export interface NarrationSegment {
  id: string;
  rawAudioPath: string;
  name: string;
  duration: number;
  startTime: number;
  inPoint: number;
  outPoint: number;
  /** Per-segment playback rate (1.0 = original; clamp 0.25–4.0). */
  playbackRate?: number;
  addedAt: number;
  /** Originating subtitle id (so re-synthesize can target the same row). */
  sourceSubtitleId?: string;
  /** All subtitle ids covered by this clip when narration was generated from an unsplit subtitle group. */
  sourceSubtitleIds?: string[];
  /** Group id for the narration generation batch this clip came from. */
  narrationBatchId?: string;
  /** Continuous group-take id shared by virtual subtitle clips from one TTS request. */
  narrationGroupTakeId?: string;
  /** Full prompt used to synthesize the continuous group audio. */
  narrationGroupPromptText?: string;
  /** Project time where the continuous group audio should start. */
  narrationGroupSourceStartTime?: number;
  /** Alignment source for the virtual subtitle boundary. */
  narrationAlignmentMode?: "aligned" | "mixed" | "estimated" | "single" | "failed";
  /** 0..1 confidence for this subtitle boundary inside the group take. */
  narrationAlignmentConfidence?: number;
  /** TTS profile snapshot used to synthesize this clip. */
  ttsProfileSnapshot?: TtsProfileSnapshot;
}

export interface TtsProfileSnapshot {
  method: string;
  geminiModel?: string;
  geminiVoice?: string;
  geminiSpeed?: string;
  geminiInstruction?: string;
  googleSpeed?: string;
  edgeVoice?: string;
  edgePitch?: number;
  edgeRate?: number;
  edgeVoiceConfigs?: Array<{
    languageCode: string;
    languageName: string;
    voiceName: string;
  }>;
  stepAudioVoice?: string;
  stepAudioReferenceVoiceId?: string;
  stepAudioPromptText?: string;
  stepAudioUseCustomReference?: boolean;
  stepAudioReferenceAudioPath?: string;
  stepAudioReferenceText?: string;
  stepAudioReferenceLabel?: string;
  magpieVoice?: string;
  magpieVoiceConfigs?: Array<{
    languageCode: string;
    languageName: string;
    voiceId: string;
  }>;
  kokoroVoice?: string;
  kokoroSpeed?: number;
  kokoroNumThreads?: number;
  kokoroVoiceConfigs?: Array<{
    languageCode: string;
    languageName: string;
    voiceId: string;
  }>;
  supertonicSpeed?: number;
  supertonicNumSteps?: number;
  supertonicNumThreads?: number;
  supertonicVoiceConfigs?: Array<{
    languageCode: string;
    languageName: string;
    voiceId: string;
  }>;
}

export type RecordingMode = "withoutCursor" | "withCursor" | "imported";

export type WebcamPosition =
  | "bottomRight"
  | "bottomLeft"
  | "topRight"
  | "topLeft";

export interface WebcamConfig {
  visible: boolean;
  position: WebcamPosition;
  mirror: boolean;
  roundnessPx: number;
  maxSizePercent: number;
  minSizePercent: number;
  autoSizeDuringZoom: boolean;
  shadowPx: number;
  insetPx: number;
}

export interface AutoZoomConfig {
  /** Follow tightness 0–1 (0 = floaty/cinematic, 1 = strict follow). Default 0.5 */
  followTightness: number;
  /** Base zoom level 1.2–4.0. Default 2.0 */
  zoomLevel: number;
  /** Speed sensitivity 0–1 (how much fast cursor movement reduces zoom). Default 0.5 */
  speedSensitivity: number;
}

export const DEFAULT_AUTO_ZOOM_CONFIG: AutoZoomConfig = {
  followTightness: 0.5,
  zoomLevel: 2.0,
  speedSensitivity: 0.5,
};

export interface VideoSegment {
  mediaMode?: "video" | "timelineOnly";
  trimStart: number;
  trimEnd: number;
  trimSegments?: TrimSegment[];
  /** @deprecated legacy point-keyframe model — migrated to `zoomBlocks` on load. */
  zoomKeyframes: ZoomKeyframe[];
  zoomBlocks?: ZoomBlock[];
  smoothMotionPath?: { time: number; x: number; y: number; zoom: number }[];
  zoomInfluencePoints?: { time: number; value: number }[];
  textSegments: TextSegment[];
  subtitleTracks?: SubtitleTrack[];
  activeSubtitleView?: SubtitleViewState;
  subtitleCustomChain?: SubtitleChainItem[];
  subtitleSegments?: SubtitleSegment[];
  cursorVisibilitySegments?: CursorVisibilitySegment[];
  keystrokeMode?: KeystrokeMode;
  keystrokeLanguage?: "en" | "ko" | "vi" | "es" | "ja" | "zh";
  keystrokeDelaySec?: number;
  keystrokeEvents?: KeystrokeEvent[];
  keyboardVisibilitySegments?: CursorVisibilitySegment[];
  keyboardMouseVisibilitySegments?: CursorVisibilitySegment[];
  keystrokeOverlay?: KeystrokeOverlayConfig;
  speedPoints?: SpeedPoint[];
  deviceAudioPoints?: DeviceAudioPoint[];
  micAudioPoints?: MicAudioPoint[];
  micAudioOffsetSec?: number;
  webcamVisibilitySegments?: CursorVisibilitySegment[];
  deviceAudioAvailable?: boolean;
  micAudioAvailable?: boolean;
  webcamOffsetSec?: number;
  webcamAvailable?: boolean;
  useCustomCursor?: boolean;
  crop?: CropRect;
}

export type ProjectCompositionMode = "separate" | "unified";

export interface ProjectCanvasConfig {
  canvasMode?: "auto" | "custom";
  canvasWidth?: number;
  canvasHeight?: number;
  autoSourceClipId?: string | null;
}

export interface ProjectCompositionClip {
  id: string;
  role: "root" | "snapshot";
  name: string;
  duration: number;
  sourceProjectId?: string;
  sourceProjectName?: string;
  thumbnail?: string;
  segment: VideoSegment;
  backgroundConfig: BackgroundConfig;
  webcamConfig?: WebcamConfig;
  mousePositions: MousePosition[];
  recordingMode?: RecordingMode;
  rawVideoPath?: string;
  rawMicAudioPath?: string;
  rawWebcamVideoPath?: string;
}

export interface ProjectComposition {
  mode: ProjectCompositionMode;
  selectedClipId: string | null;
  focusedClipId: string | null;
  clips: ProjectCompositionClip[];
  unifiedSourceClipId?: string | null;
  globalCanvasConfig?: ProjectCanvasConfig;
  globalPresentationConfig?: BackgroundConfig;
  globalSegment?: VideoSegment;
  globalBackgroundConfig?: BackgroundConfig;
  /**
   * User-supplied audio files placed on the project-wide Audio track.
   * Empty/undefined means no Audio track is rendered.
   */
  audioSegments?: ImportedAudioSegment[];
  /**
   * Track-global volume envelope for the Audio track (mirrors device audio).
   * Time domain: project-relative seconds. Volume range: 0..1.
   */
  audioTrackVolumePoints?: AudioGainPoint[];
  /**
   * TTS-generated narration clips placed on the project-wide Narration track.
   * Independent from `audioSegments`; rendered and exported in its own pass.
   */
  narrationSegments?: NarrationSegment[];
  /**
   * Track-global volume envelope for the Narration track.
   * Same shape as `audioTrackVolumePoints`.
   */
  narrationTrackVolumePoints?: AudioGainPoint[];
  /**
   * Set true when the root video is a generated silent placeholder whose only
   * job is to provide normal video timing/export behavior for imported audio.
   */
  placeholderVideoForAudio?: boolean;
  /** Marker for imported subtitle projects backed by a generated placeholder video. */
  placeholderVideoForSubtitles?: boolean;
  /**
   * Project has no source media; preview/export are driven directly by
   * timeline duration and overlays.
   * Legacy only. New subtitle imports create a generated placeholder video instead.
   */
  timelineOnly?: boolean;
}

export interface BackgroundConfig {
  scale: number;
  borderRadius: number;
  backgroundType:
    | "solid"
    | "white"
    | "gradient1"
    | "gradient2"
    | "gradient3"
    | "gradient4"
    | "gradient5"
    | "gradient6"
    | "gradient7"
    | "gradient8"
    | "gradient9"
    | "gradient10"
    | "gradient11"
    | "gradient12"
    | "gradient13"
    | "gradient14"
    | "gradient15"
    | "custom";
  shadow?: number;
  cursorScale?: number;
  cursorShadow?: number; // 0-200
  cursorSmoothness?: number;
  cursorMovementDelay?: number; // seconds, positive values make cursor lead slightly
  cursorWiggleStrength?: number; // 0-1, strength of spring follow effect
  cursorWiggleDamping?: number; // 0-1, lower = more wobble
  cursorWiggleResponse?: number; // Hz-ish response speed of spring
  cursorTiltAngle?: number; // degrees, static resting tilt offset (CCW positive)
  motionBlurCursor?: number; // 0-100 intensity (default 25, 0=off, 100=extreme)
  motionBlurZoom?: number; // 0-100 intensity
  motionBlurPan?: number; // 0-100 intensity
  backgroundZoomWithVideo?: boolean; // true = background follows video zoom/pan, false = fixed canvas background
  cursorPack?: CursorPack;
  cursorDefaultVariant?: CursorPack;
  cursorTextVariant?: CursorPack;
  cursorPointerVariant?: CursorPack;
  cursorOpenHandVariant?: CursorPack;
  customBackground?: string;
  cropBottom?: number; // 0-100 percentage
  volume?: number; // Legacy project compatibility only; replaced by segment.deviceAudioPoints
  canvasMode?: "auto" | "custom"; // default 'auto'
  canvasWidth?: number; // pixels, used when canvasMode === 'custom'
  canvasHeight?: number; // pixels, used when canvasMode === 'custom'
  autoCanvasSourceId?: string | null;
}

export interface MousePosition {
  x: number;
  y: number;
  timestamp: number;
  isClicked?: boolean;
  cursor_type?: string;
  cursor_rotation?: number; // radians, tip-anchored tail lag rotation
  captureWidth?: number;
  captureHeight?: number;
}

export interface Project {
  id: string;
  name: string;
  createdAt: number;
  lastModified: number;
  duration?: number;
  videoBlob?: Blob;
  audioBlob?: Blob;
  micAudioBlob?: Blob;
  webcamBlob?: Blob;
  segment: VideoSegment;
  backgroundConfig: BackgroundConfig;
  webcamConfig?: WebcamConfig;
  mousePositions: MousePosition[];
  thumbnail?: string;
  recordingMode?: RecordingMode;
  rawVideoPath?: string;
  rawMicAudioPath?: string;
  rawWebcamVideoPath?: string;
  composition?: ProjectComposition;
}
