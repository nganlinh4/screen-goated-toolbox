import type {
  BackgroundConfig,
  VideoSegment,
  SpeedPoint,
} from '@/types/video';

// Standard video heights (descending) for resolution options
export const STANDARD_HEIGHTS = [2160, 1440, 1080, 720, 480] as const;
export const TARGET_VIDEO_BITS_PER_PIXEL = 0.09;
export const DEFAULT_AUDIO_BITRATE_KBPS = 192;
export const MIN_VIDEO_BITRATE_KBPS = 600;
export const MAX_VIDEO_BITRATE_KBPS = 80000;
export const ESTIMATE_CALIBRATION_STORAGE_KEY = 'sr-export-estimate-calibration-v1';
export const MAX_CALIBRATION_SAMPLES = 24;
export const MAX_CALIBRATION_BUCKETS = 48;

export interface ExportEstimateCalibration {
  ratio: number;
  samples: number;
  updatedAt: number;
}

export interface ExportEstimateCalibrationStore {
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

export function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}

export function toRoundedStep(value: number, step: number): number {
  return Math.round(value / step) * step;
}

export function createDefaultCalibration(): ExportEstimateCalibration {
  return {
    ratio: 1,
    samples: 0,
    updatedAt: Date.now()
  };
}

export function normalizeCalibrationEntry(entry: Partial<ExportEstimateCalibration> | undefined): ExportEstimateCalibration {
  const fallback = createDefaultCalibration();
  if (!entry) return fallback;
  return {
    ratio: clamp(Number(entry.ratio) || 1, 0.5, 1.5),
    samples: clamp(Math.round(Number(entry.samples) || 0), 0, MAX_CALIBRATION_SAMPLES),
    updatedAt: Number(entry.updatedAt) || fallback.updatedAt
  };
}

export function readEstimateCalibrationStore(): ExportEstimateCalibrationStore {
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

export function writeEstimateCalibrationStore(store: ExportEstimateCalibrationStore) {
  try {
    if (typeof window === 'undefined' || !window.localStorage) return;
    window.localStorage.setItem(ESTIMATE_CALIBRATION_STORAGE_KEY, JSON.stringify(store));
  } catch {
    // Ignore localStorage failures (private mode, quota, etc.).
  }
}

export function blendCalibration(previous: ExportEstimateCalibration, observedRatio: number): ExportEstimateCalibration {
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

export function getMotionComplexityHint(backgroundConfig?: BackgroundConfig): number {
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

export function getTimelineComplexityHint(segment?: VideoSegment | null): number {
  if (!segment) return 0;
  const zoomBoost = clamp((segment.zoomKeyframes?.length ?? 0) / 18, 0, 1) * 0.07;
  const textBoost = clamp((segment.textSegments?.length ?? 0) / 12, 0, 1) * 0.06;
  const pointerBoost = clamp((segment.cursorVisibilitySegments?.length ?? 0) / 18, 0, 1) * 0.04;
  const keyBoost = clamp((segment.keystrokeEvents?.length ?? 0) / 350, 0, 1) * 0.05;
  return clamp(zoomBoost + textBoost + pointerBoost + keyBoost, 0, 0.20);
}

export function getResolutionBucket(width: number, height: number): string {
  const pixels = width * height;
  if (pixels <= 1280 * 720) return 'sd';
  if (pixels <= 1920 * 1080) return 'hd';
  if (pixels <= 2560 * 1440) return 'qhd';
  return 'uhd';
}

export function getFpsBucket(fps: number): string {
  const rounded = Math.round(fps);
  if (rounded <= 24) return '24';
  if (rounded <= 30) return '30';
  if (rounded <= 60) return '60';
  return 'hi';
}

export function getSpeedBucket(speed: number): string {
  if (speed < 0.85) return 'slow';
  if (speed > 1.15) return 'fast';
  return 'norm';
}

export function getBlurBucket(backgroundConfig?: BackgroundConfig): string {
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

export function getTimelineBucket(segment?: VideoSegment | null): string {
  const hint = getTimelineComplexityHint(segment);
  if (hint < 0.05) return 'low';
  if (hint < 0.12) return 'med';
  return 'high';
}

export function getTargetRatioBucket(targetVideoBitrateKbps: number, width: number, height: number, fps: number): string {
  const suggested = computeSuggestedVideoBitrateKbps(width, height, fps);
  const ratio = targetVideoBitrateKbps / Math.max(1, suggested);
  if (ratio < 0.8) return 'low';
  if (ratio > 1.2) return 'high';
  return 'mid';
}

export function getDurationBucket(outputDurationSec?: number): string {
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

export function getSpeedAtTime(time: number, points: SpeedPoint[]): number {
  if (!points || points.length === 0) return 1.0;
  const sorted = [...points].sort((a, b) => a.time - b.time);
  const idx = sorted.findIndex((p) => p.time >= time);
  if (idx === -1) return sorted[sorted.length - 1].speed;
  if (idx === 0) return sorted[0].speed;
  const p1 = sorted[idx - 1];
  const p2 = sorted[idx];
  const ratio = (time - p1.time) / Math.max(0.0001, p2.time - p1.time);
  const cosT = (1 - Math.cos(ratio * Math.PI)) / 2;
  return p1.speed + (p2.speed - p1.speed) * cosT;
}

export function calculateOutputDuration(segment: VideoSegment | null, fallbackDuration: number): number {
  if (!segment) return Math.max(0, fallbackDuration);
  const trimSegments = (
    segment.trimSegments && segment.trimSegments.length > 0
      ? segment.trimSegments
      : [{ id: 'default', startTime: segment.trimStart, endTime: segment.trimEnd }]
  )
    .map((s) => ({ startTime: Number(s.startTime) || 0, endTime: Number(s.endTime) || 0 }))
    .filter((s) => s.endTime > s.startTime)
    .sort((a, b) => a.startTime - b.startTime);
  if (trimSegments.length === 0) return Math.max(0, fallbackDuration);
  const points = segment.speedPoints || [];
  let duration = 0;
  for (const seg of trimSegments) {
    let t = seg.startTime;
    while (t < seg.endTime) {
      const dt = Math.min(0.01666, seg.endTime - t); // ~60fps integration step
      const s = getSpeedAtTime(t + dt / 2, points);
      duration += dt / Math.max(0.1, s);
      t += dt;
    }
  }
  return duration;
}

/** Estimate encoded file size for VBR output (with calibration feedback from previous exports). */
export function estimateExportSize(params: {
  width: number;
  height: number;
  fps: number;
  targetVideoBitrateKbps: number;
  trimmedDurationSec: number;
  hasAudio?: boolean;
  audioBitrateKbps?: number;
  backgroundConfig?: BackgroundConfig;
  segment?: VideoSegment | null;
  calibrationProfileKey?: string;
  calibration?: ExportEstimateCalibrationSnapshot;
}): ExportSizeEstimate {
  const outputDurationSec = calculateOutputDuration(params.segment ?? null, params.trimmedDurationSec);
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
    speed: 1, // variable speed curve uses integrated output duration
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
export function computeResolutionOptions(baseW: number, baseH: number, sourceH?: number): ResolutionOption[] {
  const aspect = baseW / baseH;
  const options: ResolutionOption[] = [];

  // "Original" is always first
  const origW = baseW % 2 === 0 ? baseW : baseW - 1;
  const origH = baseH % 2 === 0 ? baseH : baseH - 1;
  options.push({ width: origW, height: origH, label: `Original (${origW} × ${origH})` });

  const maxAllowedH = Math.max(baseH, sourceH || 0, 2160); // Allow up to 4K if aspect accommodates

  // Add standard heights
  for (const h of STANDARD_HEIGHTS) {
    if (h === origH) continue;
    if (h > maxAllowedH) continue;
    let w = Math.round(h * aspect);
    if (w % 2 !== 0) w--;
    if (w < 2) continue;

    // Avoid duplicates if the rounded width/height match original
    if (w === origW && h === origH) continue;

    const tag = h === 2160 ? '4K' : h === 1440 ? '2K' : h === 1080 ? '1080p' : h === 720 ? '720p' : '480p';
    options.push({ width: w, height: h, label: `${tag} (${w} × ${h})` });
  }

  // Sort by height descending, but keep Original first
  const stdOptions = options.slice(1).sort((a, b) => b.height - a.height);
  return [options[0], ...stdOptions];
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
