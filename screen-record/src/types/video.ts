import type { CursorPack } from "@/lib/renderer/cursorModel";

// Resolution/FPS options are computed dynamically from canvas dimensions

export interface ZoomKeyframe {
  time: number;
  duration: number;
  zoomFactor: number;
  positionX: number;
  positionY: number;
  easingType: "linear" | "easeOut" | "easeInOut";
}

export interface TextBackground {
  enabled: boolean;
  color: string;
  opacity: number; // 0-1, background pill opacity
  paddingX: number;
  paddingY: number;
  borderRadius: number;
}

export interface TextSegment {
  id: string;
  startTime: number;
  endTime: number;
  text: string;
  style: {
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
    background?: TextBackground;
  };
}

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

export type RecordingMode = "withoutCursor" | "withCursor";

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
  trimStart: number;
  trimEnd: number;
  trimSegments?: TrimSegment[];
  zoomKeyframes: ZoomKeyframe[];
  smoothMotionPath?: { time: number; x: number; y: number; zoom: number }[];
  zoomInfluencePoints?: { time: number; value: number }[];
  textSegments: TextSegment[];
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

export interface VideoMetadata {
  total_chunks: number;
  duration: number;
  width: number;
  height: number;
}

// Baked camera path
export interface BakedCameraFrame {
  time: number;
  x: number; // Global pixel X
  y: number; // Global pixel Y
  zoom: number;
}

// NEW: Baked cursor path
export interface BakedCursorFrame {
  time: number;
  x: number;
  y: number;
  scale: number; // For click squish effect
  isClicked: boolean;
  type: string;
  opacity: number; // Cursor visibility (0-1)
  rotation?: number; // radians, tip-anchored tail lag rotation
}

export interface BakedTextOverlay {
  startTime: number;
  endTime: number;
  x: number; // pixel x of bitmap top-left in output canvas
  y: number; // pixel y of bitmap top-left in output canvas
  width: number; // bitmap width
  height: number; // bitmap height
  data: number[] | string; // raw RGBA bytes or base64-encoded RGBA
}

export interface BakedKeystrokeOverlay {
  startTime: number;
  endTime: number;
  x: number;
  y: number;
  width: number;
  height: number;
  data: number[] | string;
}

export interface OverlayQuad {
  x: number;
  y: number;
  w: number;
  h: number; // screen coords (pixels)
  u: number;
  v: number;
  uw: number;
  vh: number; // atlas UVs (0..1)
  alpha: number;
}

export interface OverlayFrame {
  time?: number;
  frameIndex?: number;
  quads: OverlayQuad[];
}

export interface BakedOverlayPayload {
  atlasBase64: string;
  atlasRgba?: Uint8Array;
  atlasWidth: number;
  atlasHeight: number;
  frames: OverlayFrame[];
  totalFrameCount?: number;
  // Compact atlas metadata for Rust-side frame quad generation.
  // When present, Rust generates overlay frames instead of JS.
  atlasMetadata?: Record<string, unknown> | null;
}

export interface BakedWebcamFrame {
  time: number;
  visible: boolean;
  opacity: number;
  x: number;
  y: number;
  width: number;
  height: number;
  roundnessPx: number;
  shadowPx: number;
  mirror: boolean;
}

export interface ExportOptions {
  width: number; // 0 = use original canvas dimensions
  height: number; // 0 = use original canvas dimensions
  fps: number; // export framerate (common presets + source framerate)
  targetVideoBitrateKbps: number;
  speed?: number; // Deprecated, kept for backward compatibility if needed
  exportProfile?: "balanced" | "max_speed" | "quality_strict" | "turbo_nv";
  preferNvTurbo?: boolean;
  qualityGatePercent?: number;
  turboCodec?: "hevc" | "h264";
  preRenderPolicy?: "off" | "idle_only" | "aggressive";
  exportDiagnostics?: boolean;
  outputDir?: string;
  format?: "mp4" | "gif" | "both";
  video?: HTMLVideoElement;
  canvas?: HTMLCanvasElement;
  tempCanvas?: HTMLCanvasElement;
  segment?: VideoSegment;
  backgroundConfig?: BackgroundConfig;
  mousePositions?: MousePosition[];
  onProgress?: (progress: number) => void;
  audio?: HTMLAudioElement;
  webcamVideo?: HTMLVideoElement;
  webcamConfig?: WebcamConfig;
  bakedPath?: BakedCameraFrame[];
  bakedCursorPath?: BakedCursorFrame[];
  bakedKeystrokeOverlays?: BakedKeystrokeOverlay[];
  bakedWebcamFrames?: BakedWebcamFrame[];
}

export type ExportArtifactFormat = "mp4" | "gif";

export interface ExportArtifact {
  format: ExportArtifactFormat;
  path: string;
  bytes?: number;
  primary?: boolean;
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
