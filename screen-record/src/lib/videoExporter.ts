import type {
  ExportOptions,
  BackgroundConfig,
  BakedCameraFrame,
  BakedCursorFrame,
  BakedKeystrokeOverlay,
  BakedTextOverlay,
  MousePosition,
  VideoSegment,
} from '@/types/video';

import { videoRenderer } from './videoRenderer';
import { getTotalTrimDuration, getTrimBounds, normalizeSegmentTrimData } from './trimSegments';

// Standard video heights (descending) for resolution options
const STANDARD_HEIGHTS = [2160, 1440, 1080, 720, 480] as const;
const TARGET_VIDEO_BITS_PER_PIXEL = 0.09;
export const DEFAULT_AUDIO_BITRATE_KBPS = 192;
export const MIN_VIDEO_BITRATE_KBPS = 600;
export const MAX_VIDEO_BITRATE_KBPS = 80000;
const ESTIMATE_CALIBRATION_STORAGE_KEY = 'sr-export-estimate-calibration-v1';
const MAX_CALIBRATION_SAMPLES = 24;
const MAX_CALIBRATION_BUCKETS = 48;

interface ExportEstimateCalibration {
  ratio: number;
  samples: number;
  updatedAt: number;
}

interface ExportEstimateCalibrationStore {
  version: 2;
  global: ExportEstimateCalibration;
  buckets: Record<string, ExportEstimateCalibration>;
}

export interface ExportEstimateCalibrationSnapshot {
  ratio: number;
  samples: number;
  profileKey?: string;
  globalRatio?: number;
  globalSamples?: number;
  bucketRatio?: number;
  bucketSamples?: number;
}

export interface ResolutionOption {
  width: number;
  height: number;
  label: string;
}

export interface BitrateSliderBounds {
  minKbps: number;
  maxKbps: number;
  stepKbps: number;
  recommendedKbps: number;
}

export interface ExportSizeEstimate {
  outputDurationSec: number;
  estimatedBytes: number;
  minBytes: number;
  maxBytes: number;
  targetVideoBitrateKbps: number;
  expectedVideoBitrateKbps: number;
  audioBitrateKbps: number;
  variability: number;
  calibrationSamples: number;
  calibrationRatio: number;
  profileKey: string;
}

function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}

function toRoundedStep(value: number, step: number): number {
  return Math.round(value / step) * step;
}

function createDefaultCalibration(): ExportEstimateCalibration {
  return {
    ratio: 1,
    samples: 0,
    updatedAt: Date.now()
  };
}

function normalizeCalibrationEntry(entry: Partial<ExportEstimateCalibration> | undefined): ExportEstimateCalibration {
  const fallback = createDefaultCalibration();
  if (!entry) return fallback;
  return {
    ratio: clamp(Number(entry.ratio) || 1, 0.5, 1.5),
    samples: clamp(Math.round(Number(entry.samples) || 0), 0, MAX_CALIBRATION_SAMPLES),
    updatedAt: Number(entry.updatedAt) || fallback.updatedAt
  };
}

function readEstimateCalibrationStore(): ExportEstimateCalibrationStore {
  const fallbackGlobal = createDefaultCalibration();
  const fallbackStore: ExportEstimateCalibrationStore = {
    version: 2,
    global: fallbackGlobal,
    buckets: {}
  };
  try {
    if (typeof window === 'undefined' || !window.localStorage) return fallbackStore;
    const raw = window.localStorage.getItem(ESTIMATE_CALIBRATION_STORAGE_KEY);
    if (!raw) return fallbackStore;
    const parsed = JSON.parse(raw) as
      | (Partial<ExportEstimateCalibrationStore> & { version?: number })
      | Partial<ExportEstimateCalibration>;

    // Migrate legacy single-calibration shape.
    if (!('version' in parsed)) {
      return {
        version: 2,
        global: normalizeCalibrationEntry(parsed as Partial<ExportEstimateCalibration>),
        buckets: {}
      };
    }

    const global = normalizeCalibrationEntry(parsed.global);
    const rawBuckets = parsed.buckets && typeof parsed.buckets === 'object'
      ? (parsed.buckets as Record<string, Partial<ExportEstimateCalibration>>)
      : {};
    const bucketEntries = Object.entries(rawBuckets)
      .map(([key, value]) => [key, normalizeCalibrationEntry(value)] as const)
      .sort((a, b) => b[1].updatedAt - a[1].updatedAt)
      .slice(0, MAX_CALIBRATION_BUCKETS);

    const buckets: Record<string, ExportEstimateCalibration> = {};
    for (const [key, value] of bucketEntries) {
      buckets[key] = value;
    }

    return { version: 2, global, buckets };
  } catch {
    return fallbackStore;
  }
}

function writeEstimateCalibrationStore(store: ExportEstimateCalibrationStore) {
  try {
    if (typeof window === 'undefined' || !window.localStorage) return;
    window.localStorage.setItem(ESTIMATE_CALIBRATION_STORAGE_KEY, JSON.stringify(store));
  } catch {
    // Ignore localStorage failures (private mode, quota, etc.).
  }
}

function blendCalibration(previous: ExportEstimateCalibration, observedRatio: number): ExportEstimateCalibration {
  const weight = previous.samples < 5 ? 0.35 : 0.2;
  return {
    ratio: clamp((previous.ratio * (1 - weight)) + (observedRatio * weight), 0.5, 1.5),
    samples: Math.min(MAX_CALIBRATION_SAMPLES, previous.samples + 1),
    updatedAt: Date.now()
  };
}

export function getExportEstimateCalibration(profileKey?: string): ExportEstimateCalibrationSnapshot {
  const store = readEstimateCalibrationStore();
  const global = store.global;
  const bucket = profileKey ? store.buckets[profileKey] : undefined;

  if (!bucket || bucket.samples <= 0) {
    const bootstrappedRatio = global.samples > 0
      ? clamp(1 + ((global.ratio - 1) * 0.15), 0.75, 1.25)
      : 1;
    return {
      ratio: bootstrappedRatio,
      samples: 0,
      profileKey,
      globalRatio: global.ratio,
      globalSamples: global.samples
    };
  }

  const bucketWeight = bucket.samples >= 2
    ? 1
    : 0.5;
  return {
    ratio: clamp((global.ratio * (1 - bucketWeight)) + (bucket.ratio * bucketWeight), 0.5, 1.5),
    samples: Math.min(MAX_CALIBRATION_SAMPLES, global.samples + bucket.samples),
    profileKey,
    globalRatio: global.ratio,
    globalSamples: global.samples,
    bucketRatio: bucket.ratio,
    bucketSamples: bucket.samples
  };
}

export function recordExportEstimateResult(expectedBytes: number, actualBytes: number, profileKey?: string) {
  if (expectedBytes <= 0 || actualBytes <= 0) return;
  const observedRatio = clamp(actualBytes / expectedBytes, 0.35, 2.5);
  const store = readEstimateCalibrationStore();
  store.global = blendCalibration(store.global, observedRatio);

  if (profileKey) {
    const previousBucket = store.buckets[profileKey] ?? createDefaultCalibration();
    store.buckets[profileKey] = blendCalibration(previousBucket, observedRatio);

    const trimmedEntries = Object.entries(store.buckets)
      .sort((a, b) => b[1].updatedAt - a[1].updatedAt)
      .slice(0, MAX_CALIBRATION_BUCKETS);
    store.buckets = {};
    for (const [key, value] of trimmedEntries) {
      store.buckets[key] = value;
    }
  }

  writeEstimateCalibrationStore(store);
}

/** Resolve output dimensions where 0x0 means "original". Always returns even dimensions. */
export function resolveExportDimensions(
  requestW: number,
  requestH: number,
  baseW: number,
  baseH: number
): { width: number; height: number } {
  let width = requestW > 0 ? requestW : baseW;
  let height = requestH > 0 ? requestH : baseH;

  if (width % 2 !== 0) width--;
  if (height % 2 !== 0) height--;
  width = Math.max(2, width);
  height = Math.max(2, height);

  return { width, height };
}

/** Baseline bitrate suggestion used for defaults and slider range. */
export function computeSuggestedVideoBitrateKbps(
  width: number,
  height: number,
  fps: number
): number {
  const kbps = (width * height * Math.max(1, fps) * TARGET_VIDEO_BITS_PER_PIXEL) / 1000;
  return toRoundedStep(clamp(kbps, MIN_VIDEO_BITRATE_KBPS, MAX_VIDEO_BITRATE_KBPS), 250);
}

/** Dynamic slider bounds anchored to output dimensions/fps. */
export function computeBitrateSliderBounds(
  width: number,
  height: number,
  fps: number
): BitrateSliderBounds {
  const recommendedKbps = computeSuggestedVideoBitrateKbps(width, height, fps);
  const minKbps = toRoundedStep(clamp(recommendedKbps * 0.35, 500, recommendedKbps), 250);
  const maxKbps = toRoundedStep(clamp(recommendedKbps * 3.0, recommendedKbps + 1000, MAX_VIDEO_BITRATE_KBPS), 250);
  return { minKbps, maxKbps, stepKbps: 250, recommendedKbps };
}

function getMotionComplexityHint(backgroundConfig?: BackgroundConfig): number {
  if (!backgroundConfig) return 0;
  const blurMax = Math.max(
    Number(backgroundConfig.motionBlurCursor || 0),
    Number(backgroundConfig.motionBlurZoom || 0),
    Number(backgroundConfig.motionBlurPan || 0)
  );
  const blurBoost = clamp(blurMax / 100, 0, 1) * 0.15;
  const customBgBoost = backgroundConfig.backgroundType === 'custom' ? 0.12 : 0;
  return clamp(blurBoost + customBgBoost, 0, 0.27);
}

function getTimelineComplexityHint(segment?: VideoSegment | null): number {
  if (!segment) return 0;
  const zoomBoost = clamp((segment.zoomKeyframes?.length ?? 0) / 18, 0, 1) * 0.07;
  const textBoost = clamp((segment.textSegments?.length ?? 0) / 12, 0, 1) * 0.06;
  const pointerBoost = clamp((segment.cursorVisibilitySegments?.length ?? 0) / 18, 0, 1) * 0.04;
  const keyBoost = clamp((segment.keystrokeEvents?.length ?? 0) / 350, 0, 1) * 0.05;
  return clamp(zoomBoost + textBoost + pointerBoost + keyBoost, 0, 0.20);
}

function getResolutionBucket(width: number, height: number): string {
  const pixels = width * height;
  if (pixels <= 1280 * 720) return 'sd';
  if (pixels <= 1920 * 1080) return 'hd';
  if (pixels <= 2560 * 1440) return 'qhd';
  return 'uhd';
}

function getFpsBucket(fps: number): string {
  const rounded = Math.round(fps);
  if (rounded <= 24) return '24';
  if (rounded <= 30) return '30';
  if (rounded <= 60) return '60';
  return 'hi';
}

function getSpeedBucket(speed: number): string {
  if (speed < 0.85) return 'slow';
  if (speed > 1.15) return 'fast';
  return 'norm';
}

function getBlurBucket(backgroundConfig?: BackgroundConfig): string {
  if (!backgroundConfig) return 'none';
  const blurMax = Math.max(
    Number(backgroundConfig.motionBlurCursor || 0),
    Number(backgroundConfig.motionBlurZoom || 0),
    Number(backgroundConfig.motionBlurPan || 0)
  );
  if (blurMax <= 8) return 'none';
  if (blurMax <= 35) return 'light';
  return 'heavy';
}

function getTimelineBucket(segment?: VideoSegment | null): string {
  const hint = getTimelineComplexityHint(segment);
  if (hint < 0.05) return 'low';
  if (hint < 0.12) return 'med';
  return 'high';
}

function getTargetRatioBucket(targetVideoBitrateKbps: number, width: number, height: number, fps: number): string {
  const suggested = computeSuggestedVideoBitrateKbps(width, height, fps);
  const ratio = targetVideoBitrateKbps / Math.max(1, suggested);
  if (ratio < 0.8) return 'low';
  if (ratio > 1.2) return 'high';
  return 'mid';
}

function getDurationBucket(outputDurationSec?: number): string {
  const sec = Math.max(0, Number(outputDurationSec) || 0);
  if (sec <= 20) return 'short';
  if (sec <= 90) return 'mid';
  if (sec <= 300) return 'long';
  return 'xlong';
}

export function getExportEstimateProfileKey(params: {
  width: number;
  height: number;
  fps: number;
  targetVideoBitrateKbps: number;
  outputDurationSec?: number;
  speed?: number;
  hasAudio?: boolean;
  backgroundConfig?: BackgroundConfig;
  segment?: VideoSegment | null;
}): string {
  const speed = clamp(params.speed ?? 1, 0.1, 10);
  const bgType = params.backgroundConfig?.backgroundType || 'none';
  return [
    `res:${getResolutionBucket(params.width, params.height)}`,
    `fps:${getFpsBucket(params.fps)}`,
    `spd:${getSpeedBucket(speed)}`,
    `aud:${params.hasAudio ? '1' : '0'}`,
    `bg:${bgType}`,
    `blur:${getBlurBucket(params.backgroundConfig)}`,
    `tl:${getTimelineBucket(params.segment)}`,
    `br:${getTargetRatioBucket(params.targetVideoBitrateKbps, params.width, params.height, params.fps)}`,
    `dur:${getDurationBucket(params.outputDurationSec)}`
  ].join('|');
}

/** Estimate encoded file size for VBR output (with calibration feedback from previous exports). */
export function estimateExportSize(params: {
  width: number;
  height: number;
  fps: number;
  targetVideoBitrateKbps: number;
  trimmedDurationSec: number;
  speed?: number;
  hasAudio?: boolean;
  audioBitrateKbps?: number;
  backgroundConfig?: BackgroundConfig;
  segment?: VideoSegment | null;
  calibrationProfileKey?: string;
  calibration?: ExportEstimateCalibrationSnapshot;
}): ExportSizeEstimate {
  const safeSpeed = clamp(params.speed ?? 1, 0.1, 10);
  const outputDurationSec = Math.max(0, params.trimmedDurationSec) / safeSpeed;
  const suggestedBitrateKbps = computeSuggestedVideoBitrateKbps(params.width, params.height, params.fps);
  const targetVideoBitrateKbps = clamp(
    params.targetVideoBitrateKbps > 0 ? params.targetVideoBitrateKbps : suggestedBitrateKbps,
    MIN_VIDEO_BITRATE_KBPS,
    MAX_VIDEO_BITRATE_KBPS
  );
  const ratioToSuggested = targetVideoBitrateKbps / Math.max(1, suggestedBitrateKbps);
  const complexityHint = clamp(
    getMotionComplexityHint(params.backgroundConfig) + getTimelineComplexityHint(params.segment),
    0,
    0.35
  );
  const profileKey = params.calibrationProfileKey || getExportEstimateProfileKey({
    width: params.width,
    height: params.height,
    fps: params.fps,
    targetVideoBitrateKbps,
    outputDurationSec,
    speed: safeSpeed,
    hasAudio: params.hasAudio,
    backgroundConfig: params.backgroundConfig,
    segment: params.segment
  });

  // VBR can under-fill high targets, but may overshoot on short/complex clips.
  // Keep long clips conservative and only boost aggressively when clips are short.
  const shortness = clamp((20 - Math.min(20, outputDurationSec)) / 20, 0, 1);
  let utilization = 0.95 + (shortness * 0.06);
  if (ratioToSuggested > 1) {
    utilization -= Math.min(0.22, Math.log2(ratioToSuggested) * 0.11);
  } else {
    utilization += Math.min(0.10, (1 - ratioToSuggested) * (0.015 + (shortness * 0.10)));
  }
  const complexityScale = 0.35 + (shortness * 0.65);
  const shortClipBoost = shortness * 0.02;
  utilization = clamp(
    utilization + (complexityHint * complexityScale) + shortClipBoost,
    0.58,
    1.20
  );

  const calibration = params.calibration ?? getExportEstimateCalibration(profileKey);
  const calibrationRatio = calibration.samples > 0
    ? clamp(calibration.ratio, 0.65, 1.35)
    : 1;

  const expectedVideoBitrateKbps = Math.round(targetVideoBitrateKbps * utilization * calibrationRatio);
  const audioBitrateKbps = params.hasAudio
    ? Math.max(0, Math.round(params.audioBitrateKbps ?? DEFAULT_AUDIO_BITRATE_KBPS))
    : 0;
  const totalKbps = expectedVideoBitrateKbps + audioBitrateKbps;
  const estimatedBytes = (outputDurationSec * totalKbps * 1000) / 8;

  let variability = 0.22;
  variability += Math.max(0, Math.log2(Math.max(1, ratioToSuggested)) * 0.08);
  variability -= Math.min(0.08, calibration.samples * 0.01);
  variability -= Math.min(0.04, complexityHint * 0.2);
  variability = clamp(variability, 0.10, 0.35);
  const minBytes = estimatedBytes * (1 - variability);
  const maxBytes = estimatedBytes * (1 + variability);

  return {
    outputDurationSec,
    estimatedBytes,
    minBytes,
    maxBytes,
    targetVideoBitrateKbps,
    expectedVideoBitrateKbps,
    audioBitrateKbps,
    variability,
    calibrationSamples: calibration.samples,
    calibrationRatio,
    profileKey
  };
}

/** Compute resolution options based on the actual canvas base dimensions. */
export function computeResolutionOptions(baseW: number, baseH: number): ResolutionOption[] {
  const aspect = baseW / baseH;
  const options: ResolutionOption[] = [];

  // "Original" is always first
  const origW = baseW % 2 === 0 ? baseW : baseW - 1;
  const origH = baseH % 2 === 0 ? baseH : baseH - 1;
  options.push({ width: origW, height: origH, label: `Original (${origW} × ${origH})` });

  // Add standard heights that are strictly smaller than original
  for (const h of STANDARD_HEIGHTS) {
    if (h >= baseH) continue;
    let w = Math.round(h * aspect);
    if (w % 2 !== 0) w--;
    const tag = h === 2160 ? '4K' : h === 1440 ? '2K' : h === 1080 ? '1080p' : h === 720 ? '720p' : '480p';
    options.push({ width: w, height: h, label: `${tag} (${w} × ${h})` });
  }

  return options;
}

/** Compute the canvas base dimensions from video + crop + custom canvas config. */
export function getCanvasBaseDimensions(
  videoWidth: number, videoHeight: number,
  segment: VideoSegment | null, backgroundConfig: BackgroundConfig | undefined
): { baseW: number; baseH: number } {
  const crop = segment?.crop || { x: 0, y: 0, width: 1, height: 1 };
  const croppedW = Math.round(videoWidth * crop.width);
  const croppedH = Math.round(videoHeight * crop.height);
  const useCustom = backgroundConfig?.canvasMode === 'custom' && backgroundConfig.canvasWidth && backgroundConfig.canvasHeight;
  return {
    baseW: useCustom ? backgroundConfig!.canvasWidth! : croppedW,
    baseH: useCustom ? backgroundConfig!.canvasHeight! : croppedH,
  };
}

interface ExportPreparationContext {
  segment: VideoSegment | null;
  normalizedSegment: VideoSegment | null;
  backgroundConfig?: BackgroundConfig;
  mousePositions?: MousePosition[];
  video: HTMLVideoElement | undefined;
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

interface PreparedBakePayload {
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
  bakedTextOverlays: BakedTextOverlay[];
  bakedKeystrokeOverlays: BakedKeystrokeOverlay[];
}

interface PreparedBakeCacheEntry {
  payload: PreparedBakePayload;
  estimatedBytes: number;
}

export class VideoExporter {
  private isExporting = false;
  private readonly prepCache = new Map<string, PreparedBakeCacheEntry>();
  private readonly prepInFlight = new Map<string, Promise<PreparedBakePayload>>();
  private readonly objectIds = new WeakMap<object, number>();
  private nextObjectId = 1;
  private prepCacheBytes = 0;

  private static readonly MAX_INLINE_MEDIA_BYTES = 128 * 1024 * 1024;
  private static readonly MAX_PREP_CACHE_BYTES = 512 * 1024 * 1024;
  private static readonly MAX_PREP_CACHE_ENTRIES = 10;

  private async yieldToUiFrame() {
    await new Promise<void>((resolve) => {
      if (typeof requestAnimationFrame === 'function') {
        requestAnimationFrame(() => resolve());
      } else {
        setTimeout(() => resolve(), 0);
      }
    });
  }

  private getObjectId(value: object | null | undefined): number {
    if (!value) return 0;
    const existing = this.objectIds.get(value);
    if (existing) return existing;
    const id = this.nextObjectId++;
    this.objectIds.set(value, id);
    return id;
  }

  private estimateOverlayDataBytes(data: number[] | string): number {
    return typeof data === 'string'
      ? Math.floor((data.length * 3) / 4)
      : data.length;
  }

  private estimatePreparedPayloadBytes(payload: PreparedBakePayload): number {
    const cameraBytes = payload.bakedPath.length * 32;
    const cursorBytes = payload.bakedCursorPath.length * 48;
    const textBytes = payload.bakedTextOverlays.reduce(
      (sum, overlay) => sum + this.estimateOverlayDataBytes(overlay.data) + 64,
      0
    );
    const keystrokeBytes = payload.bakedKeystrokeOverlays.reduce(
      (sum, overlay) => sum + this.estimateOverlayDataBytes(overlay.data) + 64,
      0
    );
    return cameraBytes + cursorBytes + textBytes + keystrokeBytes;
  }

  private prunePreparationCache(requiredBytes = 0) {
    while (
      this.prepCache.size > 0 &&
      (
        this.prepCacheBytes + requiredBytes > VideoExporter.MAX_PREP_CACHE_BYTES ||
        this.prepCache.size >= VideoExporter.MAX_PREP_CACHE_ENTRIES
      )
    ) {
      const oldestKey = this.prepCache.keys().next().value;
      if (!oldestKey) break;
      const oldest = this.prepCache.get(oldestKey);
      if (oldest) {
        this.prepCacheBytes = Math.max(0, this.prepCacheBytes - oldest.estimatedBytes);
      }
      this.prepCache.delete(oldestKey);
    }
  }

  private rememberPreparedPayload(cacheKey: string, payload: PreparedBakePayload) {
    const estimatedBytes = this.estimatePreparedPayloadBytes(payload);
    if (estimatedBytes >= VideoExporter.MAX_PREP_CACHE_BYTES) {
      return;
    }

    const existing = this.prepCache.get(cacheKey);
    if (existing) {
      this.prepCacheBytes = Math.max(0, this.prepCacheBytes - existing.estimatedBytes);
      this.prepCache.delete(cacheKey);
    }

    this.prunePreparationCache(estimatedBytes);
    this.prepCache.set(cacheKey, { payload, estimatedBytes });
    this.prepCacheBytes += estimatedBytes;
  }

  private getCachedPreparedPayload(cacheKey: string): PreparedBakePayload | null {
    const cached = this.prepCache.get(cacheKey);
    if (!cached) return null;
    this.prepCache.delete(cacheKey);
    this.prepCache.set(cacheKey, cached);
    return cached.payload;
  }

  private buildPreparationContext(options: ExportOptions & {
    audioFilePath: string;
    videoFilePath?: string;
    audio?: HTMLAudioElement | null;
  }): ExportPreparationContext {
    const {
      video,
      segment,
      backgroundConfig,
      targetVideoBitrateKbps: requestedTargetVideoBitrateKbps = 0,
    } = options;

    const normalizedSegment = segment && video
      ? normalizeSegmentTrimData(segment, video.duration || segment.trimEnd)
      : segment ?? null;

    const sourceWidth = video?.videoWidth || 1920;
    const sourceHeight = video?.videoHeight || 1080;
    const { baseW, baseH } = getCanvasBaseDimensions(sourceWidth, sourceHeight, normalizedSegment, backgroundConfig);
    const { width, height } = resolveExportDimensions(options.width, options.height, baseW, baseH);
    const fps = options.fps || 60;
    const suggestedVideoBitrateKbps = computeSuggestedVideoBitrateKbps(width, height, fps);
    const targetVideoBitrateKbps = clamp(
      requestedTargetVideoBitrateKbps > 0 ? requestedTargetVideoBitrateKbps : suggestedVideoBitrateKbps,
      MIN_VIDEO_BITRATE_KBPS,
      MAX_VIDEO_BITRATE_KBPS
    );

    const videoDurationRaw = video?.duration;
    const videoDuration = Number.isFinite(videoDurationRaw)
      ? Number(videoDurationRaw)
      : (normalizedSegment?.trimEnd || 0);
    const trimBounds = normalizedSegment
      ? getTrimBounds(normalizedSegment, videoDuration || normalizedSegment.trimEnd)
      : { trimStart: 0, trimEnd: 0 };
    const activeDuration = normalizedSegment
      ? getTotalTrimDuration(normalizedSegment, videoDuration || normalizedSegment.trimEnd)
      : 0;

    return {
      segment: segment ?? null,
      normalizedSegment,
      backgroundConfig,
      mousePositions: options.mousePositions,
      video,
      videoDuration,
      sourceWidth,
      sourceHeight,
      width,
      height,
      fps,
      targetVideoBitrateKbps,
      trimBounds,
      activeDuration
    };
  }

  private buildPreparationCacheKey(context: ExportPreparationContext): string {
    const segmentId = this.getObjectId(context.segment as object | null);
    const mouseId = this.getObjectId((context.mousePositions ?? null) as object | null);
    const backgroundId = this.getObjectId((context.backgroundConfig ?? null) as object | null);
    const segment = context.segment;
    const segmentStamp = segment
      ? [
        segment.trimStart.toFixed(4),
        segment.trimEnd.toFixed(4),
        segment.zoomKeyframes?.length || 0,
        segment.textSegments?.length || 0,
        segment.trimSegments?.length || 0,
        segment.keystrokeEvents?.length || 0,
        segment.cursorVisibilitySegments?.length || 0
      ].join(':')
      : 'none';
    const mousePositions = context.mousePositions ?? [];
    const mouseLastTs = mousePositions.length > 0 ? mousePositions[mousePositions.length - 1].timestamp : 0;
    const bg = context.backgroundConfig;
    const backgroundStamp = bg
      ? `${bg.backgroundType}:${bg.scale}:${bg.borderRadius}:${bg.cursorScale ?? 0}:${bg.motionBlurCursor ?? 0}:${bg.motionBlurZoom ?? 0}:${bg.motionBlurPan ?? 0}`
      : 'none';
    return [
      segmentId,
      segmentStamp,
      mouseId,
      `${mousePositions.length}:${mouseLastTs.toFixed(3)}`,
      backgroundId,
      backgroundStamp,
      `${context.sourceWidth}x${context.sourceHeight}`,
      `${context.width}x${context.height}`,
      `fps:${context.fps}`,
      `dur:${context.videoDuration.toFixed(4)}`,
      `trim:${context.trimBounds.trimStart.toFixed(4)}-${context.trimBounds.trimEnd.toFixed(4)}`
    ].join('|');
  }

  private async computePreparedPayload(context: ExportPreparationContext): Promise<PreparedBakePayload> {
    const { normalizedSegment } = context;

    await this.yieldToUiFrame();
    const bakedPath = normalizedSegment
      ? videoRenderer.generateBakedPath(normalizedSegment, context.sourceWidth, context.sourceHeight, context.fps)
      : [];

    await this.yieldToUiFrame();
    const bakedCursorPath = normalizedSegment && context.mousePositions
      ? videoRenderer.generateBakedCursorPath(normalizedSegment, context.mousePositions, context.backgroundConfig, context.fps)
      : [];

    await this.yieldToUiFrame();
    const bakedTextOverlays = normalizedSegment
      ? videoRenderer.bakeTextOverlays(normalizedSegment, context.width, context.height)
      : [];

    await this.yieldToUiFrame();
    const bakedKeystrokeOverlays = normalizedSegment
      ? videoRenderer.bakeKeystrokeOverlays(normalizedSegment, context.width, context.height, context.fps)
      : [];

    return {
      normalizedSegment,
      sourceWidth: context.sourceWidth,
      sourceHeight: context.sourceHeight,
      width: context.width,
      height: context.height,
      fps: context.fps,
      trimBounds: context.trimBounds,
      activeDuration: context.activeDuration,
      bakedPath,
      bakedCursorPath,
      bakedTextOverlays,
      bakedKeystrokeOverlays
    };
  }

  private async getPreparedPayload(context: ExportPreparationContext): Promise<PreparedBakePayload> {
    const cacheKey = this.buildPreparationCacheKey(context);
    const cached = this.getCachedPreparedPayload(cacheKey);
    if (cached) return cached;

    const inFlight = this.prepInFlight.get(cacheKey);
    if (inFlight) {
      return inFlight;
    }

    const promise = this.computePreparedPayload(context)
      .then((payload) => {
        this.rememberPreparedPayload(cacheKey, payload);
        return payload;
      })
      .finally(() => {
        this.prepInFlight.delete(cacheKey);
      });

    this.prepInFlight.set(cacheKey, promise);
    return promise;
  }

  async primeExportPreparation(options: ExportOptions & {
    audioFilePath: string;
    videoFilePath?: string;
    audio?: HTMLAudioElement | null;
  }) {
    if (this.isExporting) return;
    if (!options.video || !options.segment) return;
    const context = this.buildPreparationContext(options);
    await this.getPreparedPayload(context);
  }

  async exportAndDownload(options: ExportOptions & {
    audioFilePath: string;
    videoFilePath?: string;
    audio?: HTMLAudioElement | null;
  }) {
    if (this.isExporting) {
      throw new Error('Export already in progress');
    }
    this.isExporting = true;
    await this.yieldToUiFrame();

    try {
      const {
        speed = 1,
        audioFilePath,
        videoFilePath,
        audio
      } = options;
      const context = this.buildPreparationContext(options);
      const prepared = await this.getPreparedPayload(context);

      // Convert media blobs to arrays for Rust only when we do not have a native source path.
      // Large recordings should flow by file path to avoid huge JS allocations.
      let videoDataArray: number[] | null = null;
      let audioDataArray: number[] | null = null;
      const sourceVideoPath = (videoFilePath || '').trim();

      if (!sourceVideoPath && context.video && context.video.src && context.video.src.startsWith('blob:')) {
        try {
          const resp = await fetch(context.video.src);
          const blob = await resp.blob();
          if (blob.size > VideoExporter.MAX_INLINE_MEDIA_BYTES) {
            throw new Error(
              `Video blob too large for inline transfer (${Math.round(blob.size / (1024 * 1024))} MB). ` +
              'A native source file path is required for large projects.'
            );
          }
          const buffer = await blob.arrayBuffer();
          videoDataArray = Array.from(new Uint8Array(buffer));
        } catch (e) {
          console.error('Failed to extract video data', e);
          const message = e instanceof Error ? e.message : String(e);
          throw new Error(`Failed to prepare video for export: ${message}`);
        }
      }

      if (audio && audio.src && audio.src.startsWith('blob:') && !audioFilePath) {
        try {
          const resp = await fetch(audio.src);
          const blob = await resp.blob();
          const buffer = await blob.arrayBuffer();
          audioDataArray = Array.from(new Uint8Array(buffer));
        } catch (e) {
          console.error('Failed to extract audio data', e);
        }
      }

      const hasAudio = Boolean((audioFilePath || '').trim() || audioDataArray || (audio && audio.src));
      const estimateProfileKey = getExportEstimateProfileKey({
        width: prepared.width,
        height: prepared.height,
        fps: prepared.fps,
        targetVideoBitrateKbps: context.targetVideoBitrateKbps,
        outputDurationSec: prepared.activeDuration / Math.max(0.1, speed),
        speed,
        hasAudio,
        backgroundConfig: context.backgroundConfig,
        segment: prepared.normalizedSegment
      });

      const exportConfig = {
        width: prepared.width,
        height: prepared.height,
        sourceWidth: prepared.sourceWidth,
        sourceHeight: prepared.sourceHeight,
        sourceVideoPath,
        framerate: prepared.fps,
        targetVideoBitrateKbps: context.targetVideoBitrateKbps,
        audioBitrateKbps: DEFAULT_AUDIO_BITRATE_KBPS,
        exportProfile: options.exportProfile || 'balanced',
        audioPath: audioFilePath,
        outputDir: options.outputDir || '',
        trimStart: prepared.trimBounds.trimStart,
        duration: prepared.activeDuration,
        speed,
        segment: prepared.normalizedSegment,
        backgroundConfig: context.backgroundConfig,
        videoData: videoDataArray,
        audioData: audioDataArray,
        bakedPath: prepared.bakedPath,
        bakedCursorPath: prepared.bakedCursorPath,
        bakedTextOverlays: prepared.bakedTextOverlays,
        bakedKeystrokeOverlays: prepared.bakedKeystrokeOverlays
      };

      // @ts-ignore
      const { invoke } = window.__TAURI__.core;

      try {
        const res = await invoke('start_export_server', exportConfig) as {
          status?: string;
          path?: string;
          bytes?: number;
        };
        if (res?.status === 'success' && typeof res.bytes === 'number' && prepared.activeDuration > 0) {
          const uncalibrated = estimateExportSize({
            width: prepared.width,
            height: prepared.height,
            fps: prepared.fps,
            targetVideoBitrateKbps: context.targetVideoBitrateKbps,
            trimmedDurationSec: prepared.activeDuration,
            speed,
            hasAudio,
            audioBitrateKbps: DEFAULT_AUDIO_BITRATE_KBPS,
            backgroundConfig: context.backgroundConfig,
            segment: prepared.normalizedSegment,
            calibrationProfileKey: estimateProfileKey,
            calibration: { ratio: 1, samples: 0 }
          });
          recordExportEstimateResult(uncalibrated.estimatedBytes, res.bytes, estimateProfileKey);
        }
      } catch (e) {
        console.error('Native Export Failed:', e);
        throw e;
      }
    } finally {
      this.isExporting = false;
    }
  }

  async cancel() {
    // @ts-ignore
    const { invoke } = window.__TAURI__.core;
    try {
      await invoke('cancel_export');
    } catch (e) {
      console.error('cancel_export invoke failed:', e);
    }
  }
}

export const videoExporter = new VideoExporter();
