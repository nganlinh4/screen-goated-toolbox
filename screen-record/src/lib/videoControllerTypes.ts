import type {
  VideoSegment,
  BackgroundConfig,
  MousePosition,
  WebcamConfig,
} from "@/types/video";

export interface VideoControllerOptions {
  videoRef: HTMLVideoElement;
  webcamVideoRef?: HTMLVideoElement;
  deviceAudioRef?: HTMLAudioElement;
  micAudioRef?: HTMLAudioElement;
  canvasRef: HTMLCanvasElement;
  tempCanvasRef: HTMLCanvasElement;
  onTimeUpdate?: (time: number) => void;
  onPlayingChange?: (isPlaying: boolean) => void;
  onVideoReady?: (ready: boolean) => void;
  onBufferingChange?: (isBuffering: boolean) => void;
  onError?: (error: string) => void;
  onDurationChange?: (duration: number) => void;
  onMetadataLoaded?: (metadata: {
    duration: number;
    width: number;
    height: number;
  }) => void;
}

export interface VideoState {
  isPlaying: boolean;
  isReady: boolean;
  isSeeking: boolean;
  currentTime: number;
  duration: number;
}

export interface RenderOptions {
  segment: VideoSegment;
  backgroundConfig: BackgroundConfig;
  webcamConfig?: WebcamConfig;
  mousePositions: MousePosition[];
  interactiveBackgroundPreview?: boolean;
}

export const PLAYBACK_RESET_LOG_DEDUPE_MS = 800;
export const PLAYBACK_RESET_DEBUG = false;
export const SEEK_SAFETY_TIMEOUT_MS = 3000;
