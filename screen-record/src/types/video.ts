export type ExportQuality = 'original' | 'balanced';
export type DimensionPreset = 'original' | '1080p' | '720p';

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

export interface CropRect {
  x: number; // 0-1
  y: number; // 0-1
  width: number; // 0-1
  height: number; // 0-1
}

export interface VideoSegment {
  trimStart: number;
  trimEnd: number;
  zoomKeyframes: ZoomKeyframe[];
  smoothMotionPath?: { time: number; x: number; y: number; zoom: number }[];
  zoomInfluencePoints?: { time: number; value: number }[];
  textSegments: TextSegment[];
  cursorVisibilitySegments?: CursorVisibilitySegment[];
  crop?: CropRect;
}

export interface BackgroundConfig {
  scale: number;
  borderRadius: number;
  backgroundType: 'solid' | 'gradient1' | 'gradient2' | 'gradient3' | 'custom';
  shadow?: number;
  cursorScale?: number;
  cursorSmoothness?: number;
  customBackground?: string;
  cropBottom?: number; // 0-100 percentage
  volume?: number; // 0-1
}

export interface MousePosition {
  x: number;
  y: number;
  timestamp: number;
  isClicked?: boolean;
  cursor_type?: string;
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
}

export interface BakedTextOverlay {
  startTime: number;
  endTime: number;
  x: number;      // pixel x of bitmap top-left in output canvas
  y: number;      // pixel y of bitmap top-left in output canvas
  width: number;  // bitmap width
  height: number; // bitmap height
  data: number[]; // raw RGBA bytes (opacity already baked in)
}

export interface ExportOptions {
  quality?: ExportQuality;
  dimensions: DimensionPreset;
  speed: number;
  video?: HTMLVideoElement;
  canvas?: HTMLCanvasElement;
  tempCanvas?: HTMLCanvasElement;
  segment?: VideoSegment;
  backgroundConfig?: BackgroundConfig;
  mousePositions?: MousePosition[];
  onProgress?: (progress: number) => void;
  audio?: HTMLAudioElement;
  bakedPath?: BakedCameraFrame[];
  // NEW: Baked cursor
  bakedCursorPath?: BakedCursorFrame[];
}

export interface ExportPreset {
  width: number;
  height: number;
  bitrate: number;
  label: string;
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
}