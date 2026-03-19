import { useState, useRef, useEffect, useCallback, type MutableRefObject } from "react";
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
import { projectManager } from "@/lib/projectManager";
import { getCompositionClip } from "@/lib/projectComposition";
import { getMediaServerUrl } from "@/lib/mediaServer";
import { VideoController } from "@/lib/videoController";
import * as helpers from "./helpers";

export type PreloadSlotKey = "previous" | "next";

export interface ClipMediaAssets {
  videoBlob: Blob | null;
  audioBlob: Blob | null;
  micAudioBlob: Blob | null;
  webcamBlob: Blob | null;
  customBackground: string | null;
}

export interface UseClipMediaCacheParams {
  // composition state
  composition: ProjectComposition | null;
  currentProjectData: Project | null;
  // video state
  currentVideo: string | null;
  currentAudio: string | null;
  currentMicAudio: string | null;
  currentWebcamVideo: string | null;
  setCurrentVideo: (url: string | null) => void;
  setCurrentAudio: (url: string | null) => void;
  setCurrentMicAudio: (url: string | null) => void;
  setCurrentWebcamVideo: (url: string | null) => void;
  setPreviewDuration: (duration: number) => void;
  setThumbnails: (thumbnails: string[]) => void;
  generateThumbnailsForSource: (opts: {
    videoUrl: string | null;
    filePath?: string;
    segment: VideoSegment;
    deferMs?: number;
  }) => Promise<void>;
  invalidateThumbnails: () => void;
  setSegment: (segment: VideoSegment | null | ((prev: VideoSegment | null) => VideoSegment | null)) => void;
  // media element refs
  videoControllerRef: MutableRefObject<VideoController | undefined>;
  webcamVideoRef: React.RefObject<HTMLVideoElement | null>;
  canvasRef: React.RefObject<HTMLCanvasElement | null>;
  tempCanvasRef: React.RefObject<HTMLCanvasElement | null>;
  // background + webcam setters
  applyLoadedBackgroundConfig: (config: BackgroundConfig) => void;
  setWebcamConfig: (config: WebcamConfig) => void;
  setMousePositions: (positions: MousePosition[]) => void;
  setCurrentRecordingMode: (mode: RecordingMode) => void;
  // raw media path setters
  handleProjectRawVideoPathChange: (path: string) => void;
  setCurrentRawMicAudioPath: (path: string) => void;
  setCurrentRawWebcamVideoPath: (path: string) => void;
  // export path resolution needs currentProjectId
  currentProjectId: string | null;
}

export function useClipMediaCache({
  composition,
  currentProjectData,
  currentVideo,
  currentAudio,
  currentMicAudio,
  currentWebcamVideo,
  setCurrentVideo,
  setCurrentAudio,
  setCurrentMicAudio,
  setCurrentWebcamVideo,
  setPreviewDuration,
  setThumbnails,
  generateThumbnailsForSource,
  invalidateThumbnails,
  setSegment,
  videoControllerRef,
  webcamVideoRef,
  canvasRef,
  tempCanvasRef,
  applyLoadedBackgroundConfig,
  setWebcamConfig,
  setMousePositions,
  setCurrentRecordingMode,
  handleProjectRawVideoPathChange,
  setCurrentRawMicAudioPath,
  setCurrentRawWebcamVideoPath,
  currentProjectId,
}: UseClipMediaCacheParams) {
  const [loadedClipId, setLoadedClipId] = useState<string | null>(null);
  const isSwitchingCompositionClipRef = useRef(false);
  const clipAssetCacheRef = useRef<Map<string, ClipMediaAssets>>(new Map());
  const clipUrlCacheRef = useRef<
    Map<
      string,
      {
        videoUrl: string;
        audioUrl: string | null;
        micAudioUrl: string | null;
        webcamVideoUrl: string | null;
      }
    >
  >(new Map());
  const clipExportSourcePathCacheRef = useRef<Map<string, string>>(new Map());
  const clipExportMicAudioPathCacheRef = useRef<Map<string, string | null>>(
    new Map(),
  );
  const clipExportWebcamPathCacheRef = useRef<Map<string, string | null>>(
    new Map(),
  );
  const preloadedSlotClipIdsRef = useRef<Record<PreloadSlotKey, string | null>>(
    {
      previous: null,
      next: null,
    },
  );
  const clipLoadRequestSeqRef = useRef(0);
  const previousPreloadVideoRef = useRef<HTMLVideoElement | null>(null);
  const previousPreloadAudioRef = useRef<HTMLAudioElement | null>(null);
  const nextPreloadVideoRef = useRef<HTMLVideoElement | null>(null);
  const nextPreloadAudioRef = useRef<HTMLAudioElement | null>(null);

  const getPreloadRefs = useCallback(
    (slot: PreloadSlotKey) =>
      helpers.getPreloadRefs(
        slot,
        previousPreloadVideoRef,
        previousPreloadAudioRef,
        nextPreloadVideoRef,
        nextPreloadAudioRef,
      ),
    [],
  );

  const clearClipMediaCaches = useCallback(
    (options?: {
      preserveVideoUrl?: string | null;
      preserveAudioUrl?: string | null;
      preserveMicAudioUrl?: string | null;
      preserveWebcamVideoUrl?: string | null;
    }) => {
      const preservedVideoUrl = options?.preserveVideoUrl ?? null;
      const preservedAudioUrl = options?.preserveAudioUrl ?? null;
      const preservedMicAudioUrl = options?.preserveMicAudioUrl ?? null;
      const preservedWebcamVideoUrl = options?.preserveWebcamVideoUrl ?? null;

      for (const {
        videoUrl,
        audioUrl,
        micAudioUrl,
        webcamVideoUrl,
      } of clipUrlCacheRef.current.values()) {
        if (videoUrl?.startsWith("blob:") && videoUrl !== preservedVideoUrl) {
          URL.revokeObjectURL(videoUrl);
        }
        if (audioUrl?.startsWith("blob:") && audioUrl !== preservedAudioUrl) {
          URL.revokeObjectURL(audioUrl);
        }
        if (
          micAudioUrl?.startsWith("blob:") &&
          micAudioUrl !== preservedMicAudioUrl
        ) {
          URL.revokeObjectURL(micAudioUrl);
        }
        if (
          webcamVideoUrl?.startsWith("blob:") &&
          webcamVideoUrl !== preservedWebcamVideoUrl
        ) {
          URL.revokeObjectURL(webcamVideoUrl);
        }
      }
      clipAssetCacheRef.current.clear();
      clipUrlCacheRef.current.clear();
      clipExportSourcePathCacheRef.current.clear();
      clipExportMicAudioPathCacheRef.current.clear();
      clipExportWebcamPathCacheRef.current.clear();
      (["previous", "next"] as const).forEach((slot) => {
        preloadedSlotClipIdsRef.current[slot] = null;
        const { videoRef: preloadVideoRef, audioRef: preloadAudioRef } =
          getPreloadRefs(slot);
        if (preloadVideoRef.current) {
          preloadVideoRef.current.pause();
          preloadVideoRef.current.removeAttribute("src");
          preloadVideoRef.current.load();
        }
        if (preloadAudioRef.current) {
          preloadAudioRef.current.pause();
          preloadAudioRef.current.removeAttribute("src");
          preloadAudioRef.current.load();
        }
      });
    },
    [getPreloadRefs],
  );

  const loadClipAssets = useCallback(
    async (
      projectId: string,
      clipId: string,
      projectOverride?: Project | null,
      compositionOverride?: ProjectComposition | null,
    ): Promise<ClipMediaAssets | null> => {
      const project = projectOverride ?? currentProjectData;
      const nextComposition = compositionOverride ?? composition;
      if (!project || !nextComposition) return null;
      const cachedAssets = clipAssetCacheRef.current.get(clipId);
      if (cachedAssets) return cachedAssets;
      const clip = getCompositionClip(nextComposition, clipId);
      if (!clip) return null;
      const loadedAssets =
        clip.role === "root"
          ? {
              videoBlob: project.videoBlob ?? null,
              audioBlob: project.audioBlob ?? null,
              micAudioBlob: project.micAudioBlob ?? null,
              webcamBlob: project.webcamBlob ?? null,
              customBackground:
                project.backgroundConfig.customBackground ?? null,
            }
          : await projectManager.loadCompositionClipAssets(projectId, clip.id);
      clipAssetCacheRef.current.set(clipId, loadedAssets);
      return loadedAssets;
    },
    [composition, currentProjectData],
  );

  const getClipMediaUrls = useCallback(
    async (
      projectId: string,
      clipId: string,
      projectOverride?: Project | null,
      compositionOverride?: ProjectComposition | null,
    ) => {
      const nextComposition = compositionOverride ?? composition;
      const clip = getCompositionClip(nextComposition, clipId);
      if (!clip) return null;
      const cachedUrls = clipUrlCacheRef.current.get(clipId);
      if (cachedUrls) return cachedUrls;
      if (clip.rawVideoPath) {
        const videoUrl = await getMediaServerUrl(clip.rawVideoPath);
        const loadedAssets = await loadClipAssets(
          projectId,
          clipId,
          projectOverride,
          compositionOverride,
        );
        const micAudioUrl = clip.rawMicAudioPath
          ? await getMediaServerUrl(clip.rawMicAudioPath)
          : loadedAssets?.micAudioBlob
            ? URL.createObjectURL(loadedAssets.micAudioBlob)
            : null;
        const webcamVideoUrl = clip.rawWebcamVideoPath
          ? await getMediaServerUrl(clip.rawWebcamVideoPath)
          : loadedAssets?.webcamBlob
            ? URL.createObjectURL(loadedAssets.webcamBlob)
            : null;
        const nextUrls = {
          videoUrl,
          audioUrl: videoUrl,
          micAudioUrl,
          webcamVideoUrl,
        };
        clipUrlCacheRef.current.set(clipId, nextUrls);
        return nextUrls;
      }
      const loadedAssets = await loadClipAssets(
        projectId,
        clipId,
        projectOverride,
        compositionOverride,
      );
      if (!loadedAssets?.videoBlob) return null;
      const videoUrl = URL.createObjectURL(loadedAssets.videoBlob);
      const nextUrls = {
        videoUrl,
        audioUrl: loadedAssets.audioBlob
          ? URL.createObjectURL(loadedAssets.audioBlob)
          : videoUrl,
        micAudioUrl: loadedAssets.micAudioBlob
          ? URL.createObjectURL(loadedAssets.micAudioBlob)
          : null,
        webcamVideoUrl: loadedAssets.webcamBlob
          ? URL.createObjectURL(loadedAssets.webcamBlob)
          : null,
      };
      clipUrlCacheRef.current.set(clipId, nextUrls);
      return nextUrls;
    },
    [composition, loadClipAssets],
  );

  const drawPreloadedClipFrame = useCallback(
    (
      clipId: string,
      nextBackground: BackgroundConfig,
      nextComposition: ProjectComposition,
    ) => {
      helpers.drawPreloadedClipFrame(
        clipId,
        nextBackground,
        nextComposition,
        preloadedSlotClipIdsRef,
        canvasRef,
        tempCanvasRef,
        previousPreloadVideoRef,
        previousPreloadAudioRef,
        nextPreloadVideoRef,
        nextPreloadAudioRef,
      );
    },
    [canvasRef, tempCanvasRef],
  );

  const primePreloadSlot = useCallback(
    async (
      slot: PreloadSlotKey,
      clipId: string | null,
      projectOverride?: Project | null,
      compositionOverride?: ProjectComposition | null,
    ) => {
      const project = projectOverride ?? currentProjectData;
      const nextComposition = compositionOverride ?? composition;
      if (!project) return;
      await helpers.primePreloadSlot(
        slot,
        clipId,
        preloadedSlotClipIdsRef,
        previousPreloadVideoRef,
        previousPreloadAudioRef,
        nextPreloadVideoRef,
        nextPreloadAudioRef,
        nextComposition,
        (projectId, cId) => getClipMediaUrls(projectId, cId, project, nextComposition),
        project.id,
      );
    },
    [composition, currentProjectData, getClipMediaUrls],
  );

  const loadClipMediaIntoEditor = useCallback(
    async (
      projectId: string,
      clipId: string,
      projectOverride?: Project | null,
      compositionOverride?: ProjectComposition | null,
      options?: {
        preferPreloadedFrame?: boolean;
        requestId?: number;
        deferThumbnailsMs?: number;
      },
    ) => {
      const project = projectOverride ?? currentProjectData;
      const nextComposition = compositionOverride ?? composition;
      if (!project || !nextComposition) return;
      const clip = getCompositionClip(nextComposition, clipId);
      if (!clip) return;
      const clipPreviewDuration = Math.max(clip.duration || 0, clip.segment.trimEnd);
      const clipThumbnailPlaceholder = clip.thumbnail
        ? Array.from({ length: 6 }, () => clip.thumbnail!)
        : null;
      const requestId = options?.requestId ?? clipLoadRequestSeqRef.current + 1;
      clipLoadRequestSeqRef.current = requestId;
      const isLatestRequest = () => clipLoadRequestSeqRef.current === requestId;
      isSwitchingCompositionClipRef.current = true;
      try {
        await helpers.loadClipMediaIntoEditorCore({
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
          preferPreloadedFrame: options?.preferPreloadedFrame ?? false,
          deferThumbnailsMs: options?.deferThumbnailsMs ?? 120,
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
        });
      } finally {
        queueMicrotask(() => {
          if (isLatestRequest()) {
            isSwitchingCompositionClipRef.current = false;
          }
        });
      }
    },
    [
      composition,
      currentAudio,
      currentMicAudio,
      currentWebcamVideo,
      currentProjectData,
      currentVideo,
      drawPreloadedClipFrame,
      getClipMediaUrls,
      handleProjectRawVideoPathChange,
      loadClipAssets,
      setCurrentAudio,
      setCurrentMicAudio,
      setCurrentWebcamVideo,
      setCurrentVideo,
      setPreviewDuration,
      setThumbnails,
      generateThumbnailsForSource,
      applyLoadedBackgroundConfig,
      setWebcamConfig,
      setMousePositions,
      invalidateThumbnails,
      setSegment,
      setCurrentRawMicAudioPath,
      setCurrentRawWebcamVideoPath,
      videoControllerRef,
    ],
  );

  const resolveClipExportSourcePath = useCallback(
    async (clip: ProjectCompositionClip): Promise<string> => {
      if (!currentProjectId) {
        throw new Error("Project not loaded");
      }
      return helpers.resolveClipExportSourcePath(
        clip,
        currentProjectId,
        clipExportSourcePathCacheRef,
        (projectId, clipId) => loadClipAssets(projectId, clipId),
      );
    },
    [currentProjectId, loadClipAssets],
  );

  const resolveClipExportMicAudioPath = useCallback(
    async (clip: ProjectCompositionClip): Promise<string> => {
      if (!currentProjectId) {
        throw new Error("Project not loaded");
      }
      return helpers.resolveClipExportMicAudioPath(
        clip,
        currentProjectId,
        clipExportMicAudioPathCacheRef,
        (projectId, clipId) => loadClipAssets(projectId, clipId),
      );
    },
    [currentProjectId, loadClipAssets],
  );

  const resolveClipExportWebcamPath = useCallback(
    async (clip: ProjectCompositionClip): Promise<string> => {
      if (!currentProjectId) {
        throw new Error("Project not loaded");
      }
      return helpers.resolveClipExportWebcamPath(
        clip,
        currentProjectId,
        clipExportWebcamPathCacheRef,
        (projectId, clipId) => loadClipAssets(projectId, clipId),
      );
    },
    [currentProjectId, loadClipAssets],
  );

  // Detect webcam 404 / load error from any code path (clip-load, project-load, etc.)
  // and automatically disable the webcam track so the UI reflects reality.
  useEffect(() => {
    if (!currentWebcamVideo || !webcamVideoRef.current) return;
    const el = webcamVideoRef.current;
    const disable = () => {
      setCurrentWebcamVideo(null);
      setSegment((prev) =>
        prev?.webcamAvailable ? { ...prev, webcamAvailable: false } : prev,
      );
    };
    // Handle the case where the error already fired before this effect ran.
    if (el.error) {
      disable();
      return;
    }
    el.addEventListener("error", disable, { once: true });
    return () => el.removeEventListener("error", disable);
  }, [currentWebcamVideo]); // eslint-disable-line react-hooks/exhaustive-deps

  return {
    loadedClipId,
    setLoadedClipId,
    isSwitchingCompositionClipRef,
    clipAssetCacheRef,
    clipUrlCacheRef,
    clipExportSourcePathCacheRef,
    clipExportMicAudioPathCacheRef,
    clipExportWebcamPathCacheRef,
    preloadedSlotClipIdsRef,
    clipLoadRequestSeqRef,
    previousPreloadVideoRef,
    previousPreloadAudioRef,
    nextPreloadVideoRef,
    nextPreloadAudioRef,
    loadClipAssets,
    getClipMediaUrls,
    clearClipMediaCaches,
    drawPreloadedClipFrame,
    primePreloadSlot,
    loadClipMediaIntoEditor,
    resolveClipExportSourcePath,
    resolveClipExportMicAudioPath,
    resolveClipExportWebcamPath,
  };
}
