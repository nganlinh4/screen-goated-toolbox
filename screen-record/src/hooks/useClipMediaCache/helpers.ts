import type { MutableRefObject } from "react";
import { videoRenderer } from "@/lib/videoRenderer";
import {
  BackgroundConfig,
  MousePosition,
  Project,
  ProjectComposition,
  ProjectCompositionClip,
  RecordingMode,
  VideoSegment,
  WebcamConfig,
} from "@/types/video";
import {
  getCompositionClip,
  getCompositionResolvedBackgroundConfig,
  getCompositionAutoSourceClipId,
} from "@/lib/projectComposition";
import { writeBlobToTempMediaFile } from "@/lib/mediaServer";
import { cloneBackgroundConfig } from "@/lib/backgroundConfig";
import { cloneWebcamConfig } from "@/lib/webcam";
import type { VideoController } from "@/lib/videoController";
import type { PreloadSlotKey } from "./index";

// ---------------------------------------------------------------------------
// getPreloadRefs
// ---------------------------------------------------------------------------

export function getPreloadRefs(
  slot: PreloadSlotKey,
  previousPreloadVideoRef: MutableRefObject<HTMLVideoElement | null>,
  previousPreloadAudioRef: MutableRefObject<HTMLAudioElement | null>,
  nextPreloadVideoRef: MutableRefObject<HTMLVideoElement | null>,
  nextPreloadAudioRef: MutableRefObject<HTMLAudioElement | null>,
) {
  return slot === "previous"
    ? { videoRef: previousPreloadVideoRef, audioRef: previousPreloadAudioRef }
    : { videoRef: nextPreloadVideoRef, audioRef: nextPreloadAudioRef };
}

// ---------------------------------------------------------------------------
// drawPreloadedClipFrame
// ---------------------------------------------------------------------------

export function drawPreloadedClipFrame(
  clipId: string,
  nextBackground: BackgroundConfig,
  nextComposition: ProjectComposition,
  preloadedSlotClipIdsRef: MutableRefObject<Record<PreloadSlotKey, string | null>>,
  canvasRef: React.RefObject<HTMLCanvasElement | null>,
  tempCanvasRef: React.RefObject<HTMLCanvasElement | null>,
  previousPreloadVideoRef: MutableRefObject<HTMLVideoElement | null>,
  previousPreloadAudioRef: MutableRefObject<HTMLAudioElement | null>,
  nextPreloadVideoRef: MutableRefObject<HTMLVideoElement | null>,
  nextPreloadAudioRef: MutableRefObject<HTMLAudioElement | null>,
) {
  const slot = (["previous", "next"] as const).find(
    (slotKey) => preloadedSlotClipIdsRef.current[slotKey] === clipId,
  );
  if (!slot || !canvasRef.current) return;
  const canvas = canvasRef.current;
  const clip = getCompositionClip(nextComposition, clipId);
  const preloadVideoRef = getPreloadRefs(
    slot,
    previousPreloadVideoRef,
    previousPreloadAudioRef,
    nextPreloadVideoRef,
    nextPreloadAudioRef,
  ).videoRef;
  const preloadedVideo = preloadVideoRef.current;
  if (!clip || !preloadedVideo || preloadedVideo.readyState < 2) return;
  if (!tempCanvasRef.current) return;
  videoRenderer.drawFrame({
    video: preloadedVideo,
    canvas,
    tempCanvas: tempCanvasRef.current,
    segment: clip.segment,
    backgroundConfig: nextBackground,
    mousePositions: clip.mousePositions,
    currentTime: preloadedVideo.currentTime || clip.segment.trimStart,
  });
}

// ---------------------------------------------------------------------------
// primePreloadSlot
// ---------------------------------------------------------------------------

export async function primePreloadSlot(
  slot: PreloadSlotKey,
  clipId: string | null,
  preloadedSlotClipIdsRef: MutableRefObject<Record<PreloadSlotKey, string | null>>,
  previousPreloadVideoRef: MutableRefObject<HTMLVideoElement | null>,
  previousPreloadAudioRef: MutableRefObject<HTMLAudioElement | null>,
  nextPreloadVideoRef: MutableRefObject<HTMLVideoElement | null>,
  nextPreloadAudioRef: MutableRefObject<HTMLAudioElement | null>,
  composition: ProjectComposition | null,
  getClipMediaUrls: (
    projectId: string,
    clipId: string,
  ) => Promise<{
    videoUrl: string;
    audioUrl: string | null;
    micAudioUrl: string | null;
    webcamVideoUrl: string | null;
  } | null>,
  projectId: string,
) {
  const { videoRef: preloadVideoRef, audioRef: preloadAudioRef } = getPreloadRefs(
    slot,
    previousPreloadVideoRef,
    previousPreloadAudioRef,
    nextPreloadVideoRef,
    nextPreloadAudioRef,
  );
  const preloadVideo = preloadVideoRef.current;
  if (!composition || !preloadVideo) return;
  if (!clipId) {
    preloadedSlotClipIdsRef.current[slot] = null;
    preloadVideo.pause();
    preloadVideo.removeAttribute("src");
    preloadVideo.load();
    if (preloadAudioRef.current) {
      preloadAudioRef.current.pause();
      preloadAudioRef.current.removeAttribute("src");
      preloadAudioRef.current.load();
    }
    return;
  }
  const clip = getCompositionClip(composition, clipId);
  if (!clip) return;
  const clipUrls = await getClipMediaUrls(projectId, clipId);
  if (!clipUrls) return;
  preloadedSlotClipIdsRef.current[slot] = clipId;
  if (preloadVideo.src !== clipUrls.videoUrl) {
    preloadVideo.src = clipUrls.videoUrl;
    preloadVideo.preload = "auto";
    preloadVideo.load();
  }
  if (preloadVideo.readyState < 2) {
    await new Promise<void>((resolve) =>
      preloadVideo.addEventListener("loadeddata", () => resolve(), {
        once: true,
      }),
    );
  }
  const startTime = clip.segment.trimStart;
  if (Math.abs(preloadVideo.currentTime - startTime) > 0.02) {
    await new Promise<void>((resolve) => {
      const onSeeked = () => resolve();
      preloadVideo.addEventListener("seeked", onSeeked, { once: true });
      preloadVideo.currentTime = startTime;
    });
  }
  if (!preloadAudioRef.current) return;
  if (!clipUrls.audioUrl) {
    preloadAudioRef.current.removeAttribute("src");
    preloadAudioRef.current.load();
    return;
  }
  if (preloadAudioRef.current.src !== clipUrls.audioUrl) {
    preloadAudioRef.current.src = clipUrls.audioUrl;
    preloadAudioRef.current.preload = "auto";
    preloadAudioRef.current.load();
  }
}

// ---------------------------------------------------------------------------
// resolveClipExportSourcePath
// ---------------------------------------------------------------------------

export async function resolveClipExportSourcePath(
  clip: ProjectCompositionClip,
  currentProjectId: string,
  cacheRef: MutableRefObject<Map<string, string>>,
  loadClipAssets: (
    projectId: string,
    clipId: string,
  ) => Promise<{ videoBlob: Blob | null } | null>,
): Promise<string> {
  const cacheKey = `${currentProjectId}:${clip.id}`;
  const cached = cacheRef.current.get(cacheKey);
  if (cached) {
    return cached;
  }
  if (clip.rawVideoPath) {
    cacheRef.current.set(cacheKey, clip.rawVideoPath);
    return clip.rawVideoPath;
  }
  const assets = await loadClipAssets(currentProjectId, clip.id);
  if (!assets?.videoBlob) {
    throw new Error(`Clip "${clip.name}" is missing source media`);
  }
  const tempPath = await writeBlobToTempMediaFile(assets.videoBlob);
  cacheRef.current.set(cacheKey, tempPath);
  return tempPath;
}

// ---------------------------------------------------------------------------
// resolveClipExportMicAudioPath
// ---------------------------------------------------------------------------

export async function resolveClipExportMicAudioPath(
  clip: ProjectCompositionClip,
  currentProjectId: string,
  cacheRef: MutableRefObject<Map<string, string | null>>,
  loadClipAssets: (
    projectId: string,
    clipId: string,
  ) => Promise<{ micAudioBlob: Blob | null | undefined } | null>,
): Promise<string> {
  const cacheKey = `${currentProjectId}:${clip.id}`;
  const cached = cacheRef.current.get(cacheKey);
  if (cached !== undefined) {
    return cached ?? "";
  }
  if (clip.rawMicAudioPath) {
    cacheRef.current.set(cacheKey, clip.rawMicAudioPath);
    return clip.rawMicAudioPath;
  }
  const assets = await loadClipAssets(currentProjectId, clip.id);
  if (!assets?.micAudioBlob) {
    cacheRef.current.set(cacheKey, null);
    return "";
  }
  const tempPath = await writeBlobToTempMediaFile(assets.micAudioBlob);
  cacheRef.current.set(cacheKey, tempPath);
  return tempPath;
}

// ---------------------------------------------------------------------------
// resolveClipExportWebcamPath
// ---------------------------------------------------------------------------

export async function resolveClipExportWebcamPath(
  clip: ProjectCompositionClip,
  currentProjectId: string,
  cacheRef: MutableRefObject<Map<string, string | null>>,
  loadClipAssets: (
    projectId: string,
    clipId: string,
  ) => Promise<{ webcamBlob: Blob | null | undefined } | null>,
): Promise<string> {
  const cacheKey = `${currentProjectId}:${clip.id}`;
  const cached = cacheRef.current.get(cacheKey);
  if (cached !== undefined) {
    return cached ?? "";
  }
  if (clip.rawWebcamVideoPath) {
    cacheRef.current.set(cacheKey, clip.rawWebcamVideoPath);
    return clip.rawWebcamVideoPath;
  }
  const assets = await loadClipAssets(currentProjectId, clip.id);
  if (!assets?.webcamBlob) {
    cacheRef.current.set(cacheKey, null);
    return "";
  }
  const tempPath = await writeBlobToTempMediaFile(assets.webcamBlob);
  cacheRef.current.set(cacheKey, tempPath);
  return tempPath;
}

// ---------------------------------------------------------------------------
// loadClipMediaIntoEditorCore
// ---------------------------------------------------------------------------

interface ClipMediaAssets {
  videoBlob: Blob | null;
  audioBlob: Blob | null;
  micAudioBlob: Blob | null;
  webcamBlob: Blob | null;
  customBackground: string | null;
}

type ClipUrlEntry = {
  videoUrl: string;
  audioUrl: string | null;
  micAudioUrl: string | null;
  webcamVideoUrl: string | null;
};

export interface LoadClipMediaCoreParams {
  projectId: string;
  clip: ProjectCompositionClip;
  project: Project;
  nextComposition: ProjectComposition;
  clipPreviewDuration: number;
  clipThumbnailPlaceholder: string[] | null;
  isLatestRequest: () => boolean;
  clipUrlCacheRef: MutableRefObject<Map<string, ClipUrlEntry>>;
  currentVideo: string | null;
  currentAudio: string | null;
  currentMicAudio: string | null;
  currentWebcamVideo: string | null;
  webcamVideoRef: React.RefObject<HTMLVideoElement | null>;
  videoControllerRef: MutableRefObject<VideoController | undefined>;
  invalidateThumbnails: () => void;
  loadClipAssets: (
    projectId: string,
    clipId: string,
    projectOverride?: Project | null,
    compositionOverride?: ProjectComposition | null,
  ) => Promise<ClipMediaAssets | null>;
  getClipMediaUrls: (
    projectId: string,
    clipId: string,
    projectOverride?: Project | null,
    compositionOverride?: ProjectComposition | null,
  ) => Promise<ClipUrlEntry | null>;
  drawPreloadedClipFrame: (
    clipId: string,
    background: BackgroundConfig,
    composition: ProjectComposition,
  ) => void;
  preferPreloadedFrame: boolean;
  deferThumbnailsMs: number;
  setCurrentVideo: (url: string | null) => void;
  setCurrentAudio: (url: string | null) => void;
  setCurrentMicAudio: (url: string | null) => void;
  setCurrentWebcamVideo: (url: string | null) => void;
  setPreviewDuration: (d: number) => void;
  setSegment: (s: VideoSegment | null | ((prev: VideoSegment | null) => VideoSegment | null)) => void;
  setThumbnails: (t: string[]) => void;
  applyLoadedBackgroundConfig: (c: BackgroundConfig) => void;
  setWebcamConfig: (c: WebcamConfig) => void;
  setMousePositions: (p: MousePosition[]) => void;
  setCurrentRecordingMode: (m: RecordingMode) => void;
  handleProjectRawVideoPathChange: (p: string) => void;
  setCurrentRawMicAudioPath: (p: string) => void;
  setCurrentRawWebcamVideoPath: (p: string) => void;
  setLoadedClipId: (id: string) => void;
  generateThumbnailsForSource: (opts: {
    videoUrl: string | null;
    filePath?: string;
    segment: VideoSegment;
    deferMs?: number;
  }) => Promise<void>;
}

export async function loadClipMediaIntoEditorCore(
  params: LoadClipMediaCoreParams,
): Promise<void> {
  const {
    projectId,
    clip,
    project,
    nextComposition,
    clipPreviewDuration,
    clipThumbnailPlaceholder,
    isLatestRequest,
    clipUrlCacheRef,
    currentVideo,
    currentAudio,
    currentMicAudio,
    currentWebcamVideo,
    webcamVideoRef,
    videoControllerRef,
    invalidateThumbnails,
    loadClipAssets,
    getClipMediaUrls,
    drawPreloadedClipFrame,
    preferPreloadedFrame,
    deferThumbnailsMs,
    setCurrentVideo,
    setCurrentAudio,
    setCurrentMicAudio,
    setCurrentWebcamVideo,
    setPreviewDuration,
    setSegment,
    setThumbnails,
    applyLoadedBackgroundConfig,
    setWebcamConfig,
    setMousePositions,
    setCurrentRecordingMode,
    handleProjectRawVideoPathChange,
    setCurrentRawMicAudioPath,
    setCurrentRawWebcamVideoPath,
    setLoadedClipId,
    generateThumbnailsForSource,
  } = params;

  const cachedVideoUrls = new Set(
    Array.from(clipUrlCacheRef.current.values()).map((urls) => urls.videoUrl),
  );
  const cachedAudioUrls = new Set(
    Array.from(clipUrlCacheRef.current.values())
      .map((urls) => urls.audioUrl)
      .filter((url): url is string => Boolean(url)),
  );
  const cachedMicAudioUrls = new Set(
    Array.from(clipUrlCacheRef.current.values())
      .map((urls) => urls.micAudioUrl)
      .filter((url): url is string => Boolean(url)),
  );
  const cachedWebcamVideoUrls = new Set(
    Array.from(clipUrlCacheRef.current.values())
      .map((urls) => urls.webcamVideoUrl)
      .filter((url): url is string => Boolean(url)),
  );
  if (currentVideo?.startsWith("blob:") && !cachedVideoUrls.has(currentVideo)) {
    URL.revokeObjectURL(currentVideo);
  }
  if (currentAudio?.startsWith("blob:") && !cachedAudioUrls.has(currentAudio)) {
    URL.revokeObjectURL(currentAudio);
  }
  if (currentMicAudio?.startsWith("blob:") && !cachedMicAudioUrls.has(currentMicAudio)) {
    URL.revokeObjectURL(currentMicAudio);
  }
  if (currentWebcamVideo?.startsWith("blob:") && !cachedWebcamVideoUrls.has(currentWebcamVideo)) {
    URL.revokeObjectURL(currentWebcamVideo);
  }
  invalidateThumbnails();
  const loadedAssets = await loadClipAssets(projectId, clip.id, project, nextComposition);
  if (!isLatestRequest()) return;
  if (!loadedAssets?.videoBlob && !clip.rawVideoPath) return;
  const clipUrls = await getClipMediaUrls(projectId, clip.id, project, nextComposition);
  if (!isLatestRequest()) return;
  const nextBackground =
    getCompositionResolvedBackgroundConfig(nextComposition, clip.id) ??
    clip.backgroundConfig;
  const customBackground = loadedAssets?.customBackground ?? undefined;
  const resolvedBackground = {
    ...nextBackground,
    customBackground: nextBackground.customBackground ?? customBackground,
  };
  if (preferPreloadedFrame) {
    drawPreloadedClipFrame(clip.id, resolvedBackground, nextComposition);
  }
  const videoObjectUrl = await videoControllerRef.current?.loadVideo(
    clipUrls
      ? { videoUrl: clipUrls.videoUrl, debugLabel: `clip-focus:${clip.id}` }
      : { videoBlob: loadedAssets?.videoBlob ?? undefined, debugLabel: `clip-focus:${clip.id}` },
  );
  if (!isLatestRequest()) return;
  if (videoObjectUrl) {
    setCurrentVideo(videoObjectUrl);
  }
  if (clipUrls?.audioUrl || loadedAssets?.audioBlob || videoObjectUrl) {
    const audioObjectUrl = await videoControllerRef.current?.loadDeviceAudio(
      clipUrls?.audioUrl
        ? { audioUrl: clipUrls.audioUrl }
        : loadedAssets?.audioBlob
          ? { audioBlob: loadedAssets.audioBlob }
          : videoObjectUrl
            ? { audioUrl: videoObjectUrl }
            : {},
    );
    if (!isLatestRequest()) return;
    setCurrentAudio(audioObjectUrl || null);
  } else {
    setCurrentAudio(null);
  }
  if (clipUrls?.micAudioUrl || loadedAssets?.micAudioBlob) {
    const micAudioObjectUrl = await videoControllerRef.current?.loadMicAudio(
      clipUrls?.micAudioUrl
        ? { audioUrl: clipUrls.micAudioUrl }
        : { audioBlob: loadedAssets?.micAudioBlob ?? undefined },
    );
    if (!isLatestRequest()) return;
    setCurrentMicAudio(micAudioObjectUrl || null);
  } else {
    setCurrentMicAudio(null);
  }
  if (clipUrls?.webcamVideoUrl || loadedAssets?.webcamBlob) {
    const webcamVideoObjectUrl = await videoControllerRef.current?.loadWebcamVideo(
      clipUrls?.webcamVideoUrl
        ? { videoUrl: clipUrls.webcamVideoUrl }
        : { videoBlob: loadedAssets?.webcamBlob ?? undefined },
    );
    if (!isLatestRequest()) return;
    setCurrentWebcamVideo(webcamVideoObjectUrl || null);
  } else {
    setCurrentWebcamVideo(null);
    // Explicitly clear the webcam video element so the old frame
    // doesn't linger in renderFrame() compositing.
    if (webcamVideoRef.current) {
      webcamVideoRef.current.pause();
      webcamVideoRef.current.removeAttribute("src");
      webcamVideoRef.current.load();
    }
  }
  if (!isLatestRequest()) return;
  setPreviewDuration(
    Math.max(videoControllerRef.current?.duration ?? 0, clipPreviewDuration),
  );
  setSegment(clip.segment);
  if (clipThumbnailPlaceholder) {
    setThumbnails(clipThumbnailPlaceholder);
  }
  // In multi-clip, force non-source clips to "custom" canvas mode so they
  // don't re-resolve auto canvas from their own video dimensions.
  const bgToApply = cloneBackgroundConfig(resolvedBackground);
  if (nextComposition && nextComposition.clips.length > 1) {
    const autoSourceId = getCompositionAutoSourceClipId(nextComposition);
    if (autoSourceId && clip.id !== autoSourceId && bgToApply.canvasMode === "auto") {
      bgToApply.canvasMode = "custom";
      bgToApply.autoCanvasSourceId = null;
    }
  }
  applyLoadedBackgroundConfig(bgToApply);
  setWebcamConfig(cloneWebcamConfig(clip.webcamConfig));
  setMousePositions(clip.mousePositions);
  setCurrentRecordingMode(clip.recordingMode ?? "withoutCursor");
  handleProjectRawVideoPathChange(clip.rawVideoPath ?? "");
  setCurrentRawMicAudioPath(clip.rawMicAudioPath ?? "");
  setCurrentRawWebcamVideoPath(clip.rawWebcamVideoPath ?? "");
  setLoadedClipId(clip.id);
  void generateThumbnailsForSource({
    videoUrl: clipUrls?.videoUrl ?? videoObjectUrl ?? null,
    filePath: clip.rawVideoPath,
    segment: clip.segment,
    deferMs: deferThumbnailsMs,
  });
}
