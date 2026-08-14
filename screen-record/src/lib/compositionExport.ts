import { invoke } from "@/lib/ipc";
import {
  computeSuggestedVideoBitrateKbps,
  getCanvasBaseDimensions,
  MAX_VIDEO_BITRATE_KBPS,
  MIN_VIDEO_BITRATE_KBPS,
  resolveExportDimensions,
} from "@/lib/exportEstimator";
import { clamp } from "@/lib/mathUtils";
import { normalizeBackgroundConfig } from "@/lib/backgroundConfig";
import {
  getCompositionAutoSourceClipId,
  getCompositionClip,
  getCompositionResolvedBackgroundConfig,
} from "@/lib/projectComposition";
import { stageBrowserCursorSlotTiles } from "@/lib/exporterCursorTiles";
import {
  stageFrameIterableInChunks,
  stageOverlayPayload,
} from "@/lib/exportStaging";
import { buildSequenceTimeline, mergeCompositionSegmentsToSequence } from "@/lib/sequenceTimeline";
import { getTotalTrimDuration, getTrimBounds, normalizeSegmentTrimData } from "@/lib/trimSegments";
import { materializeNarrationGroupTakes } from "@/lib/narrationGroupTakes";
import { videoRenderer } from "@/lib/videoRenderer";
import { normalizeAutoCanvasSegment } from "@/lib/videoGeometry";
import {
  collectNonFiniteNumberPaths,
  collectNullPaths,
  sanitizeNativeExportValue,
} from "@/lib/videoExporterPreparation";
import { throwIfExportCancelled } from "@/lib/exportCancellation";
import type {
  BackgroundConfig,
  ExportArtifact,
  ExportOptions,
  ProjectComposition,
  ProjectCompositionClip,
  VideoSegment,
  WebcamConfig,
} from "@/types/video";

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
  deviceAudioPath: string;
  micAudioPath: string;
  webcamVideoPath: string;
  sourceWidth: number;
  sourceHeight: number;
  trimStart: number;
  duration: number;
  segment: VideoSegment;
  backgroundConfig: BackgroundConfig;
  webcamConfig?: WebcamConfig;
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
  audioSegments?: import("@/types/video").ImportedAudioSegment[];
  audioTrackVolumePoints?: import("@/types/video").AudioGainPoint[];
  narrationSegments?: import("@/types/video").NarrationSegment[];
  narrationTrackVolumePoints?: import("@/types/video").AudioGainPoint[];
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
  resolveClipMicAudioPath: (clip: ProjectCompositionClip) => Promise<string>;
  resolveClipWebcamPath: (clip: ProjectCompositionClip) => Promise<string>;
  isCancelled?: () => boolean;
}

function isLockedCanvasSize(backgroundConfig: BackgroundConfig | null | undefined): boolean {
  if (!backgroundConfig?.canvasWidth || !backgroundConfig.canvasHeight) {
    return false;
  }
  return backgroundConfig.canvasMode === "custom";
}

async function probeVideoMetadata(
  path: string,
): Promise<NativeVideoMetadataProbe> {
  return invoke<NativeVideoMetadataProbe>("probe_video_metadata", { path });
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
  const {
    composition,
    exportOptions,
    resolveClipSourcePath,
    resolveClipMicAudioPath,
    resolveClipWebcamPath,
  } = context;
  const sessionId = crypto.randomUUID();
  const isCancelled = context.isCancelled;

  try {
    throwIfExportCancelled(isCancelled);
    await invoke("clear_export_staging", { sessionId });

    const authority = await resolveAuthorityBaseDimensions(
      composition,
      resolveClipSourcePath,
    );
    throwIfExportCancelled(isCancelled);
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
    const autoSourceClipId = getCompositionAutoSourceClipId(composition);

    for (const clip of composition.clips) {
      throwIfExportCancelled(isCancelled);
      const jobId = clip.id;
      const sourceVideoPath = await resolveClipSourcePath(clip);
      const micAudioPath = await resolveClipMicAudioPath(clip);
      const webcamVideoPath = await resolveClipWebcamPath(clip);
      const metadata = await probeVideoMetadata(sourceVideoPath);
      throwIfExportCancelled(isCancelled);
      const backgroundConfig = normalizeBackgroundConfig(
        getCompositionResolvedBackgroundConfig(composition, clip.id) ??
          clip.backgroundConfig,
      );
      const canvasSegment = normalizeAutoCanvasSegment(
        clip.segment,
        backgroundConfig,
        metadata.width,
        metadata.height,
        clip.id === autoSourceClipId,
      );
      const normalizedSegment = normalizeSegmentTrimData(
        canvasSegment,
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

      try {
        throwIfExportCancelled(isCancelled);
        await stageBrowserCursorSlotTiles(backgroundConfig, { sessionId, jobId });
      } catch (error) {
        console.warn(
          "[CompositionExport] Browser cursor staging failed; native rendering will be used",
          error,
        );
      }

      const overlayPayload = await videoRenderer.bakeOverlayAtlasAndPaths(
        normalizedSegment,
        width,
        height,
        fps,
      );
      throwIfExportCancelled(isCancelled);
      await stageOverlayPayload(
        overlayPayload,
        { sessionId, jobId },
        isCancelled,
      );
      if (
        webcamVideoPath &&
        clip.webcamConfig?.visible &&
        normalizedSegment.webcamAvailable !== false
      ) {
        const webcamMetadata = await probeVideoMetadata(webcamVideoPath).catch(() => null);
        await stageFrameIterableInChunks(
          videoRenderer.iterateBakedWebcamFrames(
            normalizedSegment,
            clip.webcamConfig,
            width,
            height,
            webcamMetadata && webcamMetadata.height > 0
              ? webcamMetadata.width / webcamMetadata.height
              : undefined,
            fps,
          ),
          "webcam",
          { sessionId, jobId },
          isCancelled,
        );
      }

      clipJobs.push(sanitizeNativeExportValue({
        jobId,
        clipId: clip.id,
        clipName: clip.name,
        sourceVideoPath,
        deviceAudioPath:
          normalizedSegment.deviceAudioAvailable === false ? "" : sourceVideoPath,
        micAudioPath,
        webcamVideoPath,
        sourceWidth: metadata.width,
        sourceHeight: metadata.height,
        trimStart: trimBounds.trimStart,
        duration: activeDuration,
        segment: normalizedSegment,
        backgroundConfig,
        webcamConfig: clip.webcamConfig,
        mousePositions: clip.mousePositions,
      }));
    }

    const request = sanitizeNativeExportValue({
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
        audioSegments: context.composition.audioSegments,
        audioTrackVolumePoints: context.composition.audioTrackVolumePoints,
        narrationSegments: materializeNarrationGroupTakes(context.composition.narrationSegments),
        narrationTrackVolumePoints: context.composition.narrationTrackVolumePoints,
      } satisfies NativeCompositionExportRequest);
    const nullPaths = collectNullPaths(request);
    if (nullPaths.length > 0) {
      throw new Error(`Null fields in composition export: ${nullPaths.join(", ")}`);
    }
    const nonFinitePaths = collectNonFiniteNumberPaths(request);
    if (nonFinitePaths.length > 0) {
      throw new Error(
        `Non-finite fields in composition export: ${nonFinitePaths.join(", ")}`,
      );
    }
    throwIfExportCancelled(isCancelled);
    return invoke<NativeCompositionExportResponse>(
      "start_composition_export_server",
      request,
    );
  } finally {
    await invoke("clear_export_staging", { sessionId }).catch(() => {
      // ignore cleanup failures on the best-effort staging cache clear path
    });
  }
}
