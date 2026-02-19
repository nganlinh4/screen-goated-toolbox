// Resolution/FPS options are computed dynamically from canvas dimensions

export interface ZoomKeyframe {
  time: number;
  duration: number;
  zoomFactor: number;
  positionX: number;
  positionY: number;
  easingType: 'linear' | 'easeOut' | 'easeInOut';
}

export interface TextBackground {
  enabled: boolean;
  color: string;
  opacity: number;  // 0-1, background pill opacity
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
    x: number;  // 0-100 percentage
    y: number;  // 0-100 percentage
    fontWeight?: 'normal' | 'bold';
    fontVariations?: {
      wght?: number;  // 100-900, default 400
      wdth?: number;  // 75-125, default 100
      slnt?: number;  // -12 to 0, default 0
      ROND?: number;  // 0-100, default 0
    };
    textAlign?: 'left' | 'center' | 'right';
    opacity?: number;       // 0-1, default 1
    letterSpacing?: number; // px, default 0
    background?: TextBackground;
  };
}

export interface CursorVisibilitySegment {
  id: string;
  startTime: number;
  endTime: number;
}

export type KeystrokeMode = 'off' | 'keyboard' | 'keyboardMouse';

export interface InputModifiers {
  ctrl?: boolean;
  alt?: boolean;
  shift?: boolean;
  win?: boolean;
}

export interface RawInputEvent {
  type: 'keyboard' | 'mousedown' | 'wheel';
  timestamp: number;
  vk?: number;
  key?: string;
  btn?: 'left' | 'right' | 'middle';
  direction?: 'up' | 'down' | 'none';
  modifiers?: InputModifiers;
}

export interface KeystrokeEvent {
  id: string;
  type: 'keyboard' | 'mousedown' | 'wheel';
  startTime: number;
  endTime: number;
  label: string;
  count: number;
  isHold?: boolean;
  modifiers: InputModifiers;
  key?: string;
  btn?: 'left' | 'right' | 'middle';
  direction?: 'up' | 'down' | 'none';
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

export type RecordingMode = 'withoutCursor' | 'withCursor';

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
  keystrokeDelaySec?: number;
  keystrokeEvents?: KeystrokeEvent[];
  keyboardVisibilitySegments?: CursorVisibilitySegment[];
  keyboardMouseVisibilitySegments?: CursorVisibilitySegment[];
  keystrokeOverlay?: KeystrokeOverlayConfig;
  useCustomCursor?: boolean;
  crop?: CropRect;
}

export interface BackgroundConfig {
  scale: number;
  borderRadius: number;
  backgroundType: 'solid' | 'white' | 'gradient1' | 'gradient2' | 'gradient3' | 'gradient4' | 'gradient5' | 'gradient6' | 'gradient7' | 'custom';
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
  motionBlurZoom?: number;   // 0-100 intensity
  motionBlurPan?: number;    // 0-100 intensity
  cursorPack?: 'screenstudio' | 'macos26' | 'sgtcute' | 'sgtcool' | 'sgtai' | 'sgtpixel' | 'jepriwin11';
  cursorDefaultVariant?: 'screenstudio' | 'macos26' | 'sgtcute' | 'sgtcool' | 'sgtai' | 'sgtpixel' | 'jepriwin11';
  cursorTextVariant?: 'screenstudio' | 'macos26' | 'sgtcute' | 'sgtcool' | 'sgtai' | 'sgtpixel' | 'jepriwin11';
  cursorPointerVariant?: 'screenstudio' | 'macos26' | 'sgtcute' | 'sgtcool' | 'sgtai' | 'sgtpixel' | 'jepriwin11';
  cursorOpenHandVariant?: 'screenstudio' | 'macos26' | 'sgtcute' | 'sgtcool' | 'sgtai' | 'sgtpixel' | 'jepriwin11';
  customBackground?: string;
  cropBottom?: number; // 0-100 percentage
  volume?: number; // 0-1
  canvasMode?: 'auto' | 'custom'; // default 'auto'
  canvasWidth?: number;  // pixels, used when canvasMode === 'custom'
  canvasHeight?: number; // pixels, used when canvasMode === 'custom'
}

export interface MousePosition {
  x: number;
  y: number;
  timestamp: number;
  isClicked?: boolean;
  cursor_type?: string;
  cursor_rotation?: number; // radians, tip-anchored tail lag rotation
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
  x: number;      // pixel x of bitmap top-left in output canvas
  y: number;      // pixel y of bitmap top-left in output canvas
  width: number;  // bitmap width
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

export interface ExportOptions {
  width: number;   // 0 = use original canvas dimensions
  height: number;  // 0 = use original canvas dimensions
  fps: number;     // 24, 30, or 60
  targetVideoBitrateKbps: number;
  speed: number;
  exportProfile?: 'balanced' | 'max_speed' | 'quality_strict' | 'turbo_nv';
  preferNvTurbo?: boolean;
  qualityGatePercent?: number;
  turboCodec?: 'hevc' | 'h264';
  exportDiagnostics?: boolean;
  outputDir?: string;
  video?: HTMLVideoElement;
  canvas?: HTMLCanvasElement;
  tempCanvas?: HTMLCanvasElement;
  segment?: VideoSegment;
  backgroundConfig?: BackgroundConfig;
  mousePositions?: MousePosition[];
  onProgress?: (progress: number) => void;
  audio?: HTMLAudioElement;
  bakedPath?: BakedCameraFrame[];
  bakedCursorPath?: BakedCursorFrame[];
  bakedKeystrokeOverlays?: BakedKeystrokeOverlay[];
}

export interface Project {
  id: string;
  name: string;
  createdAt: number;
  lastModified: number;
  duration?: number;
  videoBlob: Blob;
  audioBlob?: Blob;
  segment: VideoSegment;
  backgroundConfig: BackgroundConfig;
  mousePositions: MousePosition[];
  thumbnail?: string;
  recordingMode?: RecordingMode;
  rawVideoPath?: string;
}
