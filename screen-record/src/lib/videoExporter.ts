import type {
  ExportOptions,
  BackgroundConfig,
  VideoSegment,
} from '@/types/video';

import { videoRenderer } from './videoRenderer';
import { getTotalTrimDuration, getTrimBounds, normalizeSegmentTrimData } from './trimSegments';

// Standard video heights (descending) for resolution options
const STANDARD_HEIGHTS = [2160, 1440, 1080, 720, 480] as const;

export interface ResolutionOption {
  width: number;
  height: number;
  label: string;
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

export class VideoExporter {
  private isExporting = false;
  private static readonly MAX_INLINE_MEDIA_BYTES = 128 * 1024 * 1024;

  async exportAndDownload(options: ExportOptions & {
    audioFilePath: string;
    videoFilePath?: string;
    audio?: HTMLAudioElement | null;
  }) {
    if (this.isExporting) {
      throw new Error('Export already in progress');
    }
    this.isExporting = true;

    const {
      video,
      segment,
      backgroundConfig,
      mousePositions,
      speed = 1,
      audioFilePath,
      videoFilePath,
      audio
    } = options;
    const normalizedSegment = segment && video
      ? normalizeSegmentTrimData(segment, video.duration || segment.trimEnd)
      : segment ?? null;

    const vidW = video?.videoWidth || 1920;
    const vidH = video?.videoHeight || 1080;
    const { baseW, baseH } = getCanvasBaseDimensions(vidW, vidH, normalizedSegment, backgroundConfig);

    // Resolve dimensions: 0×0 means "original"
    let width = options.width > 0 ? options.width : baseW;
    let height = options.height > 0 ? options.height : baseH;

    // Ensure even (required for ffmpeg yuv420p)
    if (width % 2 !== 0) width--;
    if (height % 2 !== 0) height--;

    const fps = options.fps || 60;

    console.log('[Exporter] Video:', vidW, '×', vidH, '→ Canvas:', baseW, '×', baseH, '→ Export:', width, '×', height, '@', fps, 'fps');

    // 1. Bake camera path
    const bakedPath = normalizedSegment ? videoRenderer.generateBakedPath(normalizedSegment, vidW, vidH, fps) : [];

    // 2. Bake cursor path
    const bakedCursorPath = normalizedSegment && mousePositions
      ? videoRenderer.generateBakedCursorPath(normalizedSegment, mousePositions, backgroundConfig, fps)
      : [];

    // 3. Bake text overlays
    const bakedTextOverlays = normalizedSegment ? videoRenderer.bakeTextOverlays(normalizedSegment, width, height) : [];
    const bakedKeystrokeOverlays = normalizedSegment
      ? videoRenderer.bakeKeystrokeOverlays(normalizedSegment, width, height)
      : [];
    console.log(
      `[Exporter] Baked ${bakedPath.length} camera, ${bakedCursorPath.length} cursor, ${bakedTextOverlays.length} text, ${bakedKeystrokeOverlays.length} keystroke`
    );

    // Convert media blobs to arrays for Rust only when we do not have a native source path.
    // Large recordings should flow by file path to avoid huge JS allocations.
    let videoDataArray: number[] | null = null;
    let audioDataArray: number[] | null = null;
    const sourceVideoPath = (videoFilePath || '').trim();

    if (!sourceVideoPath && video && video.src && video.src.startsWith('blob:')) {
      try {
        const resp = await fetch(video.src);
        const blob = await resp.blob();
        if (blob.size > VideoExporter.MAX_INLINE_MEDIA_BYTES) {
          throw new Error(
            `Video blob too large for inline transfer (${Math.round(blob.size / (1024 * 1024))} MB).`
          );
        }
        const buffer = await blob.arrayBuffer();
        videoDataArray = Array.from(new Uint8Array(buffer));
      } catch (e) {
        console.error("Failed to extract video data", e);
        throw new Error("Failed to prepare video for export");
      }
    }

    if (audio && audio.src && audio.src.startsWith('blob:') && !audioFilePath) {
      try {
        const resp = await fetch(audio.src);
        const blob = await resp.blob();
        const buffer = await blob.arrayBuffer();
        audioDataArray = Array.from(new Uint8Array(buffer));
      } catch (e) {
        console.error("Failed to extract audio data", e);
      }
    }

    const trimBounds = normalizedSegment
      ? getTrimBounds(normalizedSegment, video?.duration || normalizedSegment.trimEnd)
      : { trimStart: 0, trimEnd: 0 };
    const activeDuration = normalizedSegment
      ? getTotalTrimDuration(normalizedSegment, video?.duration || normalizedSegment.trimEnd)
      : 0;

    const exportConfig = {
      width,
      height,
      sourceWidth: vidW,
      sourceHeight: vidH,
      sourceVideoPath,
      framerate: fps,
      audioPath: audioFilePath,
      outputDir: options.outputDir || '',
      trimStart: trimBounds.trimStart,
      duration: activeDuration,
      speed,
      segment: normalizedSegment,
      backgroundConfig,
      videoData: videoDataArray,
      audioData: audioDataArray,
      bakedPath,
      bakedCursorPath,
      bakedTextOverlays,
      bakedKeystrokeOverlays
    };

    // @ts-ignore
    const { invoke } = window.__TAURI__.core;

    try {
      console.log('[Exporter] Sending to native backend...');
      const res = await invoke('start_export_server', exportConfig);
      console.log('Export Success:', res);
    } catch (e) {
      console.error("Native Export Failed:", e);
      throw e;
    } finally {
      this.isExporting = false;
    }
  }

  async cancel() {
    console.log('[Cancel] videoExporter.cancel() called');
    // @ts-ignore
    const { invoke } = window.__TAURI__.core;
    try {
      console.log('[Cancel] Sending invoke("cancel_export")...');
      const res = await invoke('cancel_export');
      console.log('[Cancel] invoke returned:', res);
    } catch (e) {
      console.error('[Cancel] invoke failed:', e);
    }
  }
}

export const videoExporter = new VideoExporter();
