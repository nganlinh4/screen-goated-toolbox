import type {
  BackgroundConfig,
  ImportedAudioSegment,
  MousePosition,
  NarrationSegment,
  VideoSegment,
  WebcamConfig,
} from "./video";

export interface VideoMetadata {
  total_chunks: number;
  duration: number;
  width: number;
  height: number;
}

export interface BakedCameraFrame {
  time: number;
  x: number;
  y: number;
  zoom: number;
}

export interface BakedCursorFrame {
  time: number;
  x: number;
  y: number;
  scale: number;
  isClicked: boolean;
  type: string;
  opacity: number;
  rotation?: number;
}

export interface BakedTextOverlay {
  startTime: number;
  endTime: number;
  x: number;
  y: number;
  width: number;
  height: number;
  data: number[] | string;
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
  h: number;
  u: number;
  v: number;
  uw: number;
  vh: number;
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
  width: number;
  height: number;
  fps: number;
  targetVideoBitrateKbps: number;
  speed?: number;
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
  audioSegments?: ImportedAudioSegment[];
  narrationSegments?: NarrationSegment[];
}

export type ExportArtifactFormat = "mp4" | "gif";

export interface ExportArtifact {
  format: ExportArtifactFormat;
  path: string;
  bytes?: number;
  primary?: boolean;
}
