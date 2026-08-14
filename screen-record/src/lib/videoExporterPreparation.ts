import type {
  BackgroundConfig,
  BakedCameraFrame,
  BakedCursorFrame,
  BakedOverlayPayload,
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
  overlayPayload?: BakedOverlayPayload;
}

export interface PreparedBakeCacheEntry {
  payload: PreparedBakePayload;
  estimatedBytes: number;
}

export function sanitizeNativeExportValue<T>(value: T): T {
  return sanitizeValue(value, false).value as T;
}

function sanitizeValue(
  value: unknown,
  insideArray: boolean,
): { value: unknown; changed: boolean } {
  if (value === null || value === undefined) {
    return { value: insideArray ? null : undefined, changed: true };
  }
  if (Array.isArray(value)) {
    let changed = false;
    const sanitized = value.map((entry) => {
      const result = sanitizeValue(entry, true);
      changed ||= result.changed;
      return result.value;
    });
    return { value: changed ? sanitized : value, changed };
  }
  if (typeof value !== 'object') return { value, changed: false };

  let changed = false;
  const sanitized: Record<string, unknown> = {};
  for (const [key, entry] of Object.entries(value)) {
    const result = sanitizeValue(entry, false);
    if (result.value === undefined) {
      changed = true;
      continue;
    }
    changed ||= result.changed;
    sanitized[key] = result.value;
  }
  return { value: changed ? sanitized : value, changed };
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
