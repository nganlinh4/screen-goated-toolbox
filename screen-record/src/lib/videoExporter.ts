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
import { getCursorPack } from './renderer/cursorTypes';
import { getCursorAssetUrl } from './renderer/cursorAssets';

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
  computeGifResolutionOptions,
  GIF_MAX_WIDTH,
  getCanvasBaseDimensions,
  getExportEstimateProfileKey,
  getExportEstimateCalibration,
  recordExportEstimateResult,
  getSpeedAtTime,
  videoTimeToWallClock,
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

type CursorPackSlug =
  | 'screenstudio'
  | 'macos26'
  | 'sgtcute'
  | 'sgtcool'
  | 'sgtai'
  | 'sgtpixel'
  | 'jepriwin11'
  | 'sgtwatermelon'
  | 'sgtfastfood'
  | 'sgtveggie'
  | 'sgtvietnam'
  | 'sgtkorea';

const CURSOR_TYPES_ORDER = [
  'default',
  'text',
  'pointer',
  'openhand',
  'closehand',
  'wait',
  'appstarting',
  'crosshair',
  'resize-ns',
  'resize-we',
  'resize-nwse',
  'resize-nesw'
] as const;

const CURSOR_PACK_ORDER: CursorPackSlug[] = [
  'screenstudio',
  'macos26',
  'sgtcute',
  'sgtcool',
  'sgtai',
  'sgtpixel',
  'jepriwin11',
  'sgtwatermelon',
  'sgtfastfood',
  'sgtveggie',
  'sgtvietnam',
  'sgtkorea',
];

const CURSOR_TILE_SIZE = 512;

interface CursorSlotPngPayload {
  slotId: number;
  pngBase64: string;
}

export class VideoExporter {
  private isExporting = false;
  private readonly prepCache = new Map<string, PreparedBakeCacheEntry>();
  private readonly prepInFlight = new Map<string, Promise<PreparedBakePayload>>();
  private readonly objectIds = new WeakMap<object, number>();
  private nextObjectId = 1;
  private prepCacheBytes = 0;

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

  private async loadImage(src: string): Promise<HTMLImageElement> {
    return new Promise((resolve, reject) => {
      const img = new Image();
      img.onload = () => resolve(img);
      img.onerror = () => reject(new Error(`Failed to load ${src}`));
      img.src = src;
    });
  }

  private buildCursorSlotId(pack: CursorPackSlug, typeIndex: number): number {
    const packIndex = CURSOR_PACK_ORDER.indexOf(pack);
    if (packIndex < 0) return -1;
    return (packIndex * CURSOR_TYPES_ORDER.length) + typeIndex;
  }

  private async buildCursorSlotTilePayload(
    pack: CursorPackSlug,
    typeName: typeof CURSOR_TYPES_ORDER[number],
    typeIndex: number,
  ): Promise<CursorSlotPngPayload | null> {
    const slotId = this.buildCursorSlotId(pack, typeIndex);
    if (slotId < 0) return null;

    const src = getCursorAssetUrl(`cursor-${typeName}-${pack}`);
    let img: HTMLImageElement;
    try {
      img = await this.loadImage(src);
    } catch {
      return null;
    }

    if (!img.complete || img.naturalWidth <= 0 || img.naturalHeight <= 0) {
      return null;
    }

    const sourceMax = Math.max(img.naturalWidth, img.naturalHeight);
    const normalizeScale = sourceMax > 96 ? (48 / sourceMax) : 1;
    const drawW = img.naturalWidth * normalizeScale;
    const drawH = img.naturalHeight * normalizeScale;
    const targetMax = Math.max(drawW, drawH);
    if (targetMax <= 0.0001) {
      return null;
    }

    const tileCanvas = document.createElement('canvas');
    tileCanvas.width = CURSOR_TILE_SIZE;
    tileCanvas.height = CURSOR_TILE_SIZE;
    const tileCtx = tileCanvas.getContext('2d');
    if (!tileCtx) {
      return null;
    }
    tileCtx.clearRect(0, 0, CURSOR_TILE_SIZE, CURSOR_TILE_SIZE);
    tileCtx.imageSmoothingEnabled = true;
    tileCtx.imageSmoothingQuality = 'high';

    const tileScale = CURSOR_TILE_SIZE / targetMax;
    const tileW = drawW * tileScale;
    const tileH = drawH * tileScale;
    const x = (CURSOR_TILE_SIZE - tileW) * 0.5;
    const y = (CURSOR_TILE_SIZE - tileH) * 0.5;
    tileCtx.drawImage(img, x, y, tileW, tileH);

    return {
      slotId,
      pngBase64: tileCanvas.toDataURL('image/png'),
    };
  }

  private async stageBrowserCursorSlotTiles(backgroundConfig?: BackgroundConfig) {
    const pack = getCursorPack(backgroundConfig) as CursorPackSlug;
    const staged = (await Promise.all(
      CURSOR_TYPES_ORDER.map((typeName, idx) =>
        this.buildCursorSlotTilePayload(pack, typeName, idx))
    )).filter((payload): payload is CursorSlotPngPayload => payload !== null);
    if (staged.length === 0) return;
    await invoke('stage_export_data', {
      dataType: 'cursor_slots_png',
      data: staged,
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

  private buildTimeSegmentStamp(
    segments: Array<{ startTime: number; endTime: number }> | undefined
  ): string {
    if (!segments || segments.length === 0) return '0:0';
    let hash = 2166136261 >>> 0;
    for (const seg of segments) {
      const startMs = Math.round(seg.startTime * 1000);
      const endMs = Math.round(seg.endTime * 1000);
      hash ^= startMs;
      hash = Math.imul(hash, 16777619) >>> 0;
      hash ^= endMs;
      hash = Math.imul(hash, 16777619) >>> 0;
    }
    return `${segments.length}:${hash.toString(16)}`;
  }

  private buildJsonHash(value: unknown): string {
    let json = '';
    try {
      json = JSON.stringify(value) ?? '';
    } catch {
      json = '';
    }
    let hash = 2166136261 >>> 0;
    for (let i = 0; i < json.length; i++) {
      hash ^= json.charCodeAt(i);
      hash = Math.imul(hash, 16777619) >>> 0;
    }
    return `${json.length}:${hash.toString(16)}`;
  }

  private buildMousePositionsStamp(positions: MousePosition[]): string {
    if (positions.length === 0) return '0:0';
    return this.buildJsonHash(positions.map((position) => ({
      x: Math.round(position.x * 1000) / 1000,
      y: Math.round(position.y * 1000) / 1000,
      timestamp: Math.round(position.timestamp * 1000) / 1000,
      isClicked: Boolean(position.isClicked),
      cursorType: position.cursor_type ?? '',
      rotation: Math.round((position.cursor_rotation ?? 0) * 10000) / 10000,
      captureWidth: Math.round((position.captureWidth ?? 0) * 1000) / 1000,
      captureHeight: Math.round((position.captureHeight ?? 0) * 1000) / 1000,
    })));
  }

  private buildSegmentContentStamp(segment: VideoSegment): string {
    return this.buildJsonHash({
      trimStart: Math.round(segment.trimStart * 1000) / 1000,
      trimEnd: Math.round(segment.trimEnd * 1000) / 1000,
      trimSegments: (segment.trimSegments ?? []).map((trim) => ({
        start: Math.round(trim.startTime * 1000) / 1000,
        end: Math.round(trim.endTime * 1000) / 1000,
      })),
      zoomKeyframes: (segment.zoomKeyframes ?? []).map((frame) => ({
        time: Math.round(frame.time * 1000) / 1000,
        duration: Math.round(frame.duration * 1000) / 1000,
        zoomFactor: Math.round(frame.zoomFactor * 10000) / 10000,
        positionX: Math.round(frame.positionX * 10000) / 10000,
        positionY: Math.round(frame.positionY * 10000) / 10000,
        easingType: frame.easingType,
      })),
      speedPoints: (segment.speedPoints ?? []).map((point) => ({
        time: Math.round(point.time * 1000) / 1000,
        speed: Math.round(point.speed * 10000) / 10000,
      })),
      textSegments: (segment.textSegments ?? []).map((text) => ({
        id: text.id,
        start: Math.round(text.startTime * 1000) / 1000,
        end: Math.round(text.endTime * 1000) / 1000,
        text: text.text,
        style: text.style,
      })),
      cursorVisibility: (segment.cursorVisibilitySegments ?? []).map((visibility) => ({
        start: Math.round(visibility.startTime * 1000) / 1000,
        end: Math.round(visibility.endTime * 1000) / 1000,
      })),
      crop: segment.crop ?? null,
      useCustomCursor: segment.useCustomCursor ?? true,
      keystrokeMode: segment.keystrokeMode ?? 'off',
      keystrokeLanguage: segment.keystrokeLanguage ?? 'en',
      keystrokeDelaySec: Math.round((segment.keystrokeDelaySec ?? 0) * 1000) / 1000,
      keystrokeOverlay: segment.keystrokeOverlay ?? null,
      keyboardVisibility: (segment.keyboardVisibilitySegments ?? []).map((visibility) => ({
        start: Math.round(visibility.startTime * 1000) / 1000,
        end: Math.round(visibility.endTime * 1000) / 1000,
      })),
      keyboardMouseVisibility: (segment.keyboardMouseVisibilitySegments ?? []).map((visibility) => ({
        start: Math.round(visibility.startTime * 1000) / 1000,
        end: Math.round(visibility.endTime * 1000) / 1000,
      })),
      keystrokeEvents: (segment.keystrokeEvents ?? []).map((event) => ({
        id: event.id,
        type: event.type,
        start: Math.round(event.startTime * 1000) / 1000,
        end: Math.round(event.endTime * 1000) / 1000,
        label: event.label,
        count: event.count,
        isHold: Boolean(event.isHold),
        key: event.key ?? '',
        btn: event.btn ?? '',
        direction: event.direction ?? '',
        modifiers: {
          ctrl: Boolean(event.modifiers?.ctrl),
          alt: Boolean(event.modifiers?.alt),
          shift: Boolean(event.modifiers?.shift),
          win: Boolean(event.modifiers?.win),
        },
      })),
    });
  }

  private buildBackgroundStamp(backgroundConfig: BackgroundConfig | undefined): string {
    if (!backgroundConfig) return 'none';
    const customBackground = backgroundConfig.customBackground ?? '';
    return this.buildJsonHash({
      ...backgroundConfig,
      customBackground: customBackground
        ? `${customBackground.length}:${customBackground.slice(0, 64)}:${customBackground.slice(-64)}`
        : '',
    });
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
        this.buildSegmentContentStamp(segment),
        this.buildTimeSegmentStamp(segment.cursorVisibilitySegments),
        this.buildTimeSegmentStamp(segment.keyboardVisibilitySegments),
        this.buildTimeSegmentStamp(segment.keyboardMouseVisibilitySegments),
      ].join(':')
      : 'none';
    const mousePositions = context.mousePositions ?? [];
    const mouseLastTs = mousePositions.length > 0 ? mousePositions[mousePositions.length - 1].timestamp : 0;
    const mouseStamp = this.buildMousePositionsStamp(mousePositions);
    const backgroundStamp = this.buildBackgroundStamp(context.backgroundConfig);
    return [
      segmentId,
      segmentStamp,
      mouseId,
      `${mousePositions.length}:${mouseLastTs.toFixed(3)}:${mouseStamp}`,
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
    const t0Total = performance.now();

    try {
      const {
        audioFilePath,
        videoFilePath,
        audio
      } = options;
      const context = this.buildPreparationContext(options);
      const prepared = await this.getPreparedPayload(context);
      const tAfterPrep = performance.now();
      console.log(`[Prep] getPreparedPayload: ${(tAfterPrep - t0Total).toFixed(0)}ms`);

      // Video/audio data always flows by file path — Rust falls back to VIDEO_PATH
      // if sourceVideoPath is empty. Never send raw bytes through JSON IPC.
      const sourceVideoPath = (videoFilePath || '').trim();

      const hasAudio = Boolean((audioFilePath || '').trim() || (audio && audio.src));
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
      try {
        await this.stageBrowserCursorSlotTiles(context.backgroundConfig);
      } catch (e) {
        console.warn('[Prep] Browser cursor tile staging failed, falling back to native rasterization:', e);
      }

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

      // Animated cursor frames are pre-staged to Rust's persistent store
      // in the background at app startup (see cursorAnimationCapture.ts).
      // Zero additional work here — export reads from the persistent store.

      // Send lightweight config (no baked arrays — they're already staged)
      const mousePositions = context.mousePositions ?? [];
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
        format: options.format || 'mp4',
        trimStart: prepared.trimBounds.trimStart,
        duration: prepared.activeDuration,
        segment: prepared.normalizedSegment,
        backgroundConfig: context.backgroundConfig,
        mousePositions,
      };
      const tBeforeStringify = performance.now();
      const configJsonSize = JSON.stringify(exportConfig).length;
      const tAfterStringify = performance.now();
      console.log(`[Prep] JSON.stringify exportConfig: ${(tAfterStringify - tBeforeStringify).toFixed(0)}ms, size=${(configJsonSize / 1024).toFixed(0)}KB, mousePositions=${mousePositions.length}`);

      try {
        const tBeforeInvoke = performance.now();
        const res = await invoke('start_export_server', exportConfig) as {
          status?: string;
          path?: string;
          bytes?: number;
          diagnostics?: ExportRuntimeDiagnostics;
        };
        const tAfterInvoke = performance.now();
        console.log(`[Prep] invoke('start_export_server') roundtrip: ${(tAfterInvoke - tBeforeInvoke).toFixed(0)}ms`);
        console.log(`[Prep] Total 'Preparing' duration: ${(tAfterInvoke - t0Total).toFixed(0)}ms`);
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
