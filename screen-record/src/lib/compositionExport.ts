import { invoke } from "@/lib/ipc";
import {
  clamp,
  computeSuggestedVideoBitrateKbps,
  getCanvasBaseDimensions,
  MAX_VIDEO_BITRATE_KBPS,
  MIN_VIDEO_BITRATE_KBPS,
  resolveExportDimensions,
} from "@/lib/exportEstimator";
import {
  getCompositionAutoSourceClipId,
  getCompositionClip,
  getCompositionResolvedBackgroundConfig,
} from "@/lib/projectComposition";
import { getCursorAssetUrl } from "@/lib/renderer/cursorAssets";
import { getCursorPack } from "@/lib/renderer/cursorTypes";
import { buildSequenceTimeline, mergeCompositionSegmentsToSequence } from "@/lib/sequenceTimeline";
import { getTotalTrimDuration, getTrimBounds, normalizeSegmentTrimData } from "@/lib/trimSegments";
import { videoRenderer } from "@/lib/videoRenderer";
import type {
  BackgroundConfig,
  ExportArtifact,
  ExportOptions,
  ProjectComposition,
  ProjectCompositionClip,
  VideoSegment,
} from "@/types/video";

type CursorPackSlug =
  | "screenstudio"
  | "macos26"
  | "sgtcute"
  | "sgtcool"
  | "sgtai"
  | "sgtpixel"
  | "jepriwin11"
  | "sgtwatermelon"
  | "sgtfastfood"
  | "sgtveggie"
  | "sgtvietnam"
  | "sgtkorea";

const CURSOR_TYPES_ORDER = [
  "default",
  "text",
  "pointer",
  "openhand",
  "closehand",
  "wait",
  "appstarting",
  "crosshair",
  "resize-ns",
  "resize-we",
  "resize-nwse",
  "resize-nesw",
] as const;

const CURSOR_PACK_ORDER: CursorPackSlug[] = [
  "screenstudio",
  "macos26",
  "sgtcute",
  "sgtcool",
  "sgtai",
  "sgtpixel",
  "jepriwin11",
  "sgtwatermelon",
  "sgtfastfood",
  "sgtveggie",
  "sgtvietnam",
  "sgtkorea",
];

const CURSOR_TILE_SIZE = 512;

interface NativeVideoMetadataProbe {
  width: number;
  height: number;
  fps: number;
  duration: number;
}

interface NativeCompositionExportClipJob {
  jobId: string;
  clipId: string;
  clipName: string;
  sourceVideoPath: string;
  audioPath: string;
  sourceWidth: number;
  sourceHeight: number;
  trimStart: number;
  duration: number;
  segment: VideoSegment;
  backgroundConfig: BackgroundConfig;
  mousePositions: ProjectCompositionClip["mousePositions"];
}

interface NativeCompositionExportRequest {
  sessionId: string;
  width: number;
  height: number;
  framerate: number;
  targetVideoBitrateKbps: number;
  qualityGatePercent: number;
  preRenderPolicy: "off" | "idle_only" | "aggressive";
  outputDir: string;
  format: "mp4" | "gif" | "both";
  clips: NativeCompositionExportClipJob[];
}

interface NativeCompositionExportResponse {
  status?: string;
  path?: string;
  artifacts?: ExportArtifact[];
}

export interface CompositionExportDialogState {
  baseWidth: number;
  baseHeight: number;
  sourceFps: number | null;
  segment: VideoSegment | null;
  backgroundConfig: BackgroundConfig | null;
  trimmedDurationSec: number;
  clipCount: number;
  hasAudio: boolean;
}

export interface CompositionExportContext {
  composition: ProjectComposition;
  exportOptions: ExportOptions;
  resolveClipSourcePath: (clip: ProjectCompositionClip) => Promise<string>;
}

function isLockedCanvasSize(backgroundConfig: BackgroundConfig | null | undefined): boolean {
  if (!backgroundConfig?.canvasWidth || !backgroundConfig.canvasHeight) {
    return false;
  }
  if (backgroundConfig.canvasMode === "custom") {
    return true;
  }
  return (
    backgroundConfig.canvasMode === "auto" &&
    !!backgroundConfig.autoCanvasSourceId
  );
}

async function probeVideoMetadata(
  path: string,
): Promise<NativeVideoMetadataProbe> {
  return invoke<NativeVideoMetadataProbe>("probe_video_metadata", { path });
}

function buildCursorSlotId(pack: CursorPackSlug, typeIndex: number): number {
  const packIndex = CURSOR_PACK_ORDER.indexOf(pack);
  if (packIndex < 0) return -1;
  return packIndex * CURSOR_TYPES_ORDER.length + typeIndex;
}

async function loadImage(src: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const image = new Image();
    image.onload = () => resolve(image);
    image.onerror = () => reject(new Error(`Failed to load ${src}`));
    image.src = src;
  });
}

async function buildCursorSlotTilePayload(
  pack: CursorPackSlug,
  typeName: (typeof CURSOR_TYPES_ORDER)[number],
  typeIndex: number,
): Promise<{ slotId: number; pngBase64: string } | null> {
  const slotId = buildCursorSlotId(pack, typeIndex);
  if (slotId < 0) return null;

  let image: HTMLImageElement;
  try {
    image = await loadImage(getCursorAssetUrl(`cursor-${typeName}-${pack}`));
  } catch {
    return null;
  }

  if (!image.complete || image.naturalWidth <= 0 || image.naturalHeight <= 0) {
    return null;
  }

  const tileCanvas = document.createElement("canvas");
  tileCanvas.width = CURSOR_TILE_SIZE;
  tileCanvas.height = CURSOR_TILE_SIZE;
  const tileCtx = tileCanvas.getContext("2d");
  if (!tileCtx) return null;

  tileCtx.clearRect(0, 0, CURSOR_TILE_SIZE, CURSOR_TILE_SIZE);
  tileCtx.imageSmoothingEnabled = true;
  tileCtx.imageSmoothingQuality = "high";

  const targetMax = Math.max(image.naturalWidth, image.naturalHeight, 1);
  const tileScale = CURSOR_TILE_SIZE / targetMax;
  const tileW = image.naturalWidth * tileScale;
  const tileH = image.naturalHeight * tileScale;
  const x = (CURSOR_TILE_SIZE - tileW) * 0.5;
  const y = (CURSOR_TILE_SIZE - tileH) * 0.5;
  tileCtx.drawImage(image, x, y, tileW, tileH);

  return {
    slotId,
    pngBase64: tileCanvas.toDataURL("image/png"),
  };
}

async function stageBrowserCursorSlotTiles(
  sessionId: string,
  jobId: string,
  backgroundConfig: BackgroundConfig,
) {
  const pack = getCursorPack(backgroundConfig) as CursorPackSlug;
  const tiles = (
    await Promise.all(
      CURSOR_TYPES_ORDER.map((typeName, typeIndex) =>
        buildCursorSlotTilePayload(pack, typeName, typeIndex),
      ),
    )
  ).filter((tile): tile is { slotId: number; pngBase64: string } => Boolean(tile));

  if (tiles.length === 0) return;

  await invoke("stage_export_data", {
    sessionId,
    jobId,
    dataType: "cursor_slots_png",
    data: tiles,
  });
}

function getAuthorityClip(
  composition: ProjectComposition,
): ProjectCompositionClip | null {
  const authorityClipId = getCompositionAutoSourceClipId(composition);
  return getCompositionClip(composition, authorityClipId);
}

async function resolveAuthorityBaseDimensions(
  composition: ProjectComposition,
  resolveClipSourcePath: (clip: ProjectCompositionClip) => Promise<string>,
): Promise<{
  baseWidth: number;
  baseHeight: number;
  sourceFps: number | null;
  backgroundConfig: BackgroundConfig | null;
}> {
  const authorityClip = getAuthorityClip(composition);
  if (!authorityClip) {
    return {
      baseWidth: 1920,
      baseHeight: 1080,
      sourceFps: null,
      backgroundConfig: null,
    };
  }

  const authorityBackground =
    getCompositionResolvedBackgroundConfig(composition, authorityClip.id) ??
    authorityClip.backgroundConfig;
  const sourcePath = await resolveClipSourcePath(authorityClip);
  const metadata = await probeVideoMetadata(sourcePath);

  if (isLockedCanvasSize(authorityBackground)) {
    return {
      baseWidth: authorityBackground.canvasWidth!,
      baseHeight: authorityBackground.canvasHeight!,
      sourceFps:
        typeof metadata.fps === "number" && Number.isFinite(metadata.fps)
          ? metadata.fps
          : null,
      backgroundConfig: authorityBackground,
    };
  }

  const { baseW, baseH } = getCanvasBaseDimensions(
    metadata.width,
    metadata.height,
    authorityClip.segment,
    authorityBackground,
  );

  return {
    baseWidth: baseW,
    baseHeight: baseH,
    sourceFps:
      typeof metadata.fps === "number" && Number.isFinite(metadata.fps)
        ? metadata.fps
        : null,
    backgroundConfig: authorityBackground,
  };
}

export async function buildCompositionExportDialogState(
  composition: ProjectComposition,
  resolveClipSourcePath: (clip: ProjectCompositionClip) => Promise<string>,
): Promise<CompositionExportDialogState> {
  const authority = await resolveAuthorityBaseDimensions(
    composition,
    resolveClipSourcePath,
  );
  const timeline = buildSequenceTimeline(composition);
  return {
    baseWidth: authority.baseWidth,
    baseHeight: authority.baseHeight,
    sourceFps: authority.sourceFps,
    segment: timeline ? mergeCompositionSegmentsToSequence(timeline) : null,
    backgroundConfig: authority.backgroundConfig,
    trimmedDurationSec: timeline?.totalDuration ?? 0,
    clipCount: composition.clips.length,
    hasAudio: composition.clips.length > 0,
  };
}

export async function exportCompositionAndDownload(
  context: CompositionExportContext,
): Promise<NativeCompositionExportResponse> {
  const { composition, exportOptions, resolveClipSourcePath } = context;
  const sessionId = crypto.randomUUID();

  try {
    await invoke("clear_export_staging", { sessionId });

    const authority = await resolveAuthorityBaseDimensions(
      composition,
      resolveClipSourcePath,
    );
    const { width, height } = resolveExportDimensions(
      exportOptions.width,
      exportOptions.height,
      authority.baseWidth,
      authority.baseHeight,
    );
    const fps = Math.max(
      1,
      Math.round(exportOptions.fps || authority.sourceFps || 60),
    );
    const targetVideoBitrateKbps = clamp(
      exportOptions.targetVideoBitrateKbps > 0
        ? exportOptions.targetVideoBitrateKbps
        : computeSuggestedVideoBitrateKbps(width, height, fps),
      MIN_VIDEO_BITRATE_KBPS,
      MAX_VIDEO_BITRATE_KBPS,
    );
    const qualityGatePercent = exportOptions.qualityGatePercent ?? 3;
    const preRenderPolicy = exportOptions.preRenderPolicy || "aggressive";
    const clipJobs: NativeCompositionExportClipJob[] = [];

    for (const clip of composition.clips) {
      const jobId = clip.id;
      const sourceVideoPath = await resolveClipSourcePath(clip);
      const metadata = await probeVideoMetadata(sourceVideoPath);
      const backgroundConfig =
        getCompositionResolvedBackgroundConfig(composition, clip.id) ??
        clip.backgroundConfig;
      const normalizedSegment = normalizeSegmentTrimData(
        clip.segment,
        metadata.duration || clip.segment.trimEnd,
      );
      const trimBounds = getTrimBounds(
        normalizedSegment,
        metadata.duration || normalizedSegment.trimEnd,
      );
      const activeDuration = getTotalTrimDuration(
        normalizedSegment,
        metadata.duration || normalizedSegment.trimEnd,
      );

      await stageBrowserCursorSlotTiles(sessionId, jobId, backgroundConfig);

      const overlayPayload = await videoRenderer.bakeOverlayAtlasAndPaths(
        normalizedSegment,
        width,
        height,
        fps,
      );
      if (overlayPayload) {
        await invoke("stage_export_data", {
          sessionId,
          jobId,
          dataType: "atlas",
          base64: overlayPayload.atlasBase64,
          width: overlayPayload.atlasWidth,
          height: overlayPayload.atlasHeight,
        });
        const frameChunkSize = 1500;
        for (
          let frameIndex = 0;
          frameIndex < overlayPayload.frames.length;
          frameIndex += frameChunkSize
        ) {
          await invoke("stage_export_data", {
            sessionId,
            jobId,
            dataType: "overlay_frames_chunk",
            data: overlayPayload.frames.slice(
              frameIndex,
              frameIndex + frameChunkSize,
            ),
          });
        }
      }

      clipJobs.push({
        jobId,
        clipId: clip.id,
        clipName: clip.name,
        sourceVideoPath,
        audioPath: sourceVideoPath,
        sourceWidth: metadata.width,
        sourceHeight: metadata.height,
        trimStart: trimBounds.trimStart,
        duration: activeDuration,
        segment: normalizedSegment,
        backgroundConfig,
        mousePositions: clip.mousePositions,
      });
    }

    return invoke<NativeCompositionExportResponse>(
      "start_composition_export_server",
      {
        sessionId,
        width,
        height,
        framerate: fps,
        targetVideoBitrateKbps,
        qualityGatePercent,
        preRenderPolicy,
        outputDir: exportOptions.outputDir || "",
        format: exportOptions.format || "mp4",
        clips: clipJobs,
      } satisfies NativeCompositionExportRequest,
    );
  } finally {
    await invoke("clear_export_staging", { sessionId }).catch(() => {
      // ignore cleanup failures on the best-effort staging cache clear path
    });
  }
}
