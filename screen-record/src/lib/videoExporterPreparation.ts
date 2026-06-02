import type {
  BackgroundConfig,
  BakedCameraFrame,
  BakedCursorFrame,
  BakedOverlayPayload,
  BakedWebcamFrame,
  MousePosition,
  VideoSegment,
  WebcamConfig,
} from '@/types/video';

export interface ExportPreparationContext {
  segment: VideoSegment | null;
  normalizedSegment: VideoSegment | null;
  backgroundConfig?: BackgroundConfig;
  webcamConfig?: WebcamConfig;
  mousePositions?: MousePosition[];
  video: HTMLVideoElement | undefined;
  webcamVideo?: HTMLVideoElement | undefined;
  videoDuration: number;
  sourceWidth: number;
  sourceHeight: number;
  width: number;
  height: number;
  fps: number;
  targetVideoBitrateKbps: number;
  trimBounds: { trimStart: number; trimEnd: number };
  activeDuration: number;
}

export interface PreparedBakePayload {
  normalizedSegment: VideoSegment | null;
  sourceWidth: number;
  sourceHeight: number;
  width: number;
  height: number;
  fps: number;
  trimBounds: { trimStart: number; trimEnd: number };
  activeDuration: number;
  bakedPath: BakedCameraFrame[];
  bakedCursorPath: BakedCursorFrame[];
  bakedWebcamFrames: BakedWebcamFrame[];
  overlayPayload?: BakedOverlayPayload;
}

export interface PreparedBakeCacheEntry {
  payload: PreparedBakePayload;
  estimatedBytes: number;
}

export function sanitizeNativeExportValue<T>(value: T): T {
  return JSON.parse(
    JSON.stringify(value, (_key, nestedValue) =>
      nestedValue === null ? undefined : nestedValue),
  ) as T;
}

export function collectNullPaths(
  value: unknown,
  basePath = '$',
  output: string[] = [],
): string[] {
  if (value === null) {
    output.push(basePath);
    return output;
  }
  if (Array.isArray(value)) {
    value.forEach((entry, index) => {
      collectNullPaths(entry, `${basePath}[${index}]`, output);
    });
    return output;
  }
  if (typeof value === 'object' && value) {
    Object.entries(value).forEach(([key, entry]) => {
      collectNullPaths(entry, `${basePath}.${key}`, output);
    });
  }
  return output;
}

export function collectNonFiniteNumberPaths(
  value: unknown,
  basePath = '$',
  output: string[] = [],
): string[] {
  if (typeof value === 'number' && !Number.isFinite(value)) {
    output.push(basePath);
    return output;
  }
  if (Array.isArray(value)) {
    value.forEach((entry, index) => {
      collectNonFiniteNumberPaths(entry, `${basePath}[${index}]`, output);
    });
    return output;
  }
  if (typeof value === 'object' && value) {
    Object.entries(value).forEach(([key, entry]) => {
      collectNonFiniteNumberPaths(entry, `${basePath}.${key}`, output);
    });
  }
  return output;
}
