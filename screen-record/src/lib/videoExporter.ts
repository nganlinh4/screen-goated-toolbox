import type {
  ExportOptions,
  BackgroundConfig,
  BakedCameraFrame,
  BakedCursorFrame,
  BakedOverlayPayload,
  MousePosition,
  VideoSegment,
} from '@/types/video';

import { videoRenderer } from './videoRenderer';
import { getTotalTrimDuration, getTrimBounds, normalizeSegmentTrimData } from './trimSegments';
import { invoke } from '@/lib/ipc';

import {
  clamp,
  DEFAULT_AUDIO_BITRATE_KBPS,
  MIN_VIDEO_BITRATE_KBPS,
  MAX_VIDEO_BITRATE_KBPS,
  computeSuggestedVideoBitrateKbps,
  resolveExportDimensions,
  getCanvasBaseDimensions,
  getExportEstimateProfileKey,
  calculateOutputDuration,
  estimateExportSize,
  recordExportEstimateResult,
} from './exportEstimator';

// Re-export everything from exportEstimator for backwards compatibility
export {
  DEFAULT_AUDIO_BITRATE_KBPS,
  MIN_VIDEO_BITRATE_KBPS,
  MAX_VIDEO_BITRATE_KBPS,
  computeSuggestedVideoBitrateKbps,
  computeBitrateSliderBounds,
  resolveExportDimensions,
  computeResolutionOptions,
  getCanvasBaseDimensions,
  getExportEstimateProfileKey,
  getExportEstimateCalibration,
  recordExportEstimateResult,
  getSpeedAtTime,
  calculateOutputDuration,
  estimateExportSize,
} from './exportEstimator';

export type {
  ExportEstimateCalibrationSnapshot,
  ResolutionOption,
  BitrateSliderBounds,
  ExportSizeEstimate,
} from './exportEstimator';

export interface ExportCapabilities {
  pipeline?: string;
  mfH264Available?: boolean;
  nvencAvailable: boolean;
  hevcNvencAvailable: boolean;
  sfeSupported: boolean;
  maxBFrames: number;
  driverVersion?: string;
  reasonIfDisabled?: string;
}

export interface ExportRuntimeDiagnostics {
  backend?: string;
  encoder?: string;
  codec?: string;
  turbo?: boolean;
  sfe?: boolean;
  preRenderPolicy?: string;
  qualityGatePercent?: number;
  actualTotalBitrateKbps?: number;
  expectedTotalBitrateKbps?: number;
  bitrateDeviationPercent?: number;
  readbackRingSize?: number;
  decodeQueueCapacity?: number;
  decodeRecycleCapacity?: number;
  writerQueueCapacity?: number;
  writerRecycleCapacity?: number;
  decodeWaitSecs?: number;
  composeRenderSecs?: number;
  readbackWaitSecs?: number;
  writerBlockSecs?: number;
  maxDecodeInflight?: number;
  maxWriterInflight?: number;
  maxPendingReadbacks?: number;
  fallbackUsed?: boolean;
  fallbackAttempts?: number;
  fallbackErrors?: string[];
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
  overlayPayload?: BakedOverlayPayload;
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

  private estimatePreparedPayloadBytes(payload: PreparedBakePayload): number {
    const cameraBytes = payload.bakedPath.length * 32;
    const cursorBytes = payload.bakedCursorPath.length * 48;
    const atlasBytes = payload.overlayPayload
      ? Math.floor((payload.overlayPayload.atlasBase64.length * 3) / 4)
      : 0;
    const framesBytes = payload.overlayPayload
      ? payload.overlayPayload.frames.reduce((sum, f) => sum + f.quads.length * 40, 0)
      : 0;
    return cameraBytes + cursorBytes + atlasBytes + framesBytes;
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

    // Camera and cursor paths are now generated in Rust from raw keyframes/mouse positions.
    const bakedPath: BakedCameraFrame[] = [];
    const bakedCursorPath: BakedCursorFrame[] = [];

    await this.yieldToUiFrame();
    const t0 = Date.now();
    const overlayPayload = normalizedSegment
      ? await videoRenderer.bakeOverlayAtlasAndPaths(normalizedSegment, context.width, context.height, context.fps)
      : undefined;
    await invoke('log_message', { message: `[Prep] Build Overlay Atlas: ${Date.now() - t0}ms` });

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
      overlayPayload,
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
    if ((options.preRenderPolicy || 'aggressive') === 'off') return;
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
        outputDurationSec: calculateOutputDuration(prepared.normalizedSegment, prepared.activeDuration),
        speed: 1,
        hasAudio,
        backgroundConfig: context.backgroundConfig,
        segment: prepared.normalizedSegment
      });

      // Stage baked data via chunked IPC to avoid V8 JSON.stringify limits.
      await invoke('clear_export_staging', {});

      // Camera and cursor baking now done in Rust — only stage text/keystroke overlays.
      // Chunked at 50 per call: avoids per-overlay IPC overhead on long recordings.
      const t0Overlays = Date.now();
      if (prepared.overlayPayload) {
        await invoke('stage_export_data', {
          dataType: 'atlas',
          base64: prepared.overlayPayload.atlasBase64,
          width: prepared.overlayPayload.atlasWidth,
          height: prepared.overlayPayload.atlasHeight,
        });
        const FRAME_CHUNK = 1500;
        const frames = prepared.overlayPayload.frames;
        for (let i = 0; i < frames.length; i += FRAME_CHUNK) {
          await invoke('stage_export_data', {
            dataType: 'overlay_frames_chunk',
            data: frames.slice(i, i + FRAME_CHUNK),
          });
        }
      }
      await invoke('log_message', { message: `[Prep] IPC Atlas+Overlays: ${Date.now() - t0Overlays}ms` });

      // Send lightweight config (no baked arrays — they're already staged)
      const exportConfig = {
        width: prepared.width,
        height: prepared.height,
        sourceWidth: prepared.sourceWidth,
        sourceHeight: prepared.sourceHeight,
        sourceVideoPath,
        framerate: prepared.fps,
        targetVideoBitrateKbps: context.targetVideoBitrateKbps,
        qualityGatePercent: options.qualityGatePercent ?? 3,
        preRenderPolicy: options.preRenderPolicy || 'aggressive',
        audioPath: audioFilePath,
        outputDir: options.outputDir || '',
        trimStart: prepared.trimBounds.trimStart,
        duration: prepared.activeDuration,
        segment: prepared.normalizedSegment,
        backgroundConfig: context.backgroundConfig,
        mousePositions: context.mousePositions ?? [],
        videoData: videoDataArray,
        audioData: audioDataArray,
      };

      try {
        const res = await invoke('start_export_server', exportConfig) as {
          status?: string;
          path?: string;
          bytes?: number;
          diagnostics?: ExportRuntimeDiagnostics;
        };
        if (res?.diagnostics) {
          window.postMessage({
            type: 'sr-export-diagnostics',
            diagnostics: res.diagnostics
          }, '*');
        }
        if (res?.status === 'success' && typeof res.bytes === 'number' && prepared.activeDuration > 0) {
          const uncalibrated = estimateExportSize({
            width: prepared.width,
            height: prepared.height,
            fps: prepared.fps,
            targetVideoBitrateKbps: context.targetVideoBitrateKbps,
            trimmedDurationSec: prepared.activeDuration,
            hasAudio,
            audioBitrateKbps: DEFAULT_AUDIO_BITRATE_KBPS,
            backgroundConfig: context.backgroundConfig,
            segment: prepared.normalizedSegment,
            calibrationProfileKey: estimateProfileKey,
            calibration: { ratio: 1, samples: 0 }
          });
          recordExportEstimateResult(uncalibrated.estimatedBytes, res.bytes, estimateProfileKey);
        }
        return res;
      } catch (e) {
        console.error('Native Export Failed:', e);
        throw e;
      }
    } finally {
      this.isExporting = false;
    }
  }

  async cancel() {
    try {
      await invoke('cancel_export');
    } catch (e) {
      console.error('cancel_export invoke failed:', e);
    }
  }

  async getExportCapabilities(): Promise<ExportCapabilities> {
    const res = await invoke<Record<string, unknown>>('get_export_capabilities');
    return {
      pipeline: typeof res?.pipeline === 'string' ? res.pipeline : undefined,
      mfH264Available: Boolean(res?.mf_h264 ?? res?.mfH264Available),
      nvencAvailable: Boolean(res?.nvencAvailable),
      hevcNvencAvailable: Boolean(res?.hevcNvencAvailable),
      sfeSupported: Boolean(res?.sfeSupported),
      maxBFrames: typeof res?.maxBFrames === 'number' ? res.maxBFrames : 0,
      driverVersion: typeof res?.driverVersion === 'string' ? res.driverVersion : undefined,
      reasonIfDisabled: typeof res?.reasonIfDisabled === 'string' ? res.reasonIfDisabled : undefined
    };
  }
}

export const videoExporter = new VideoExporter();
