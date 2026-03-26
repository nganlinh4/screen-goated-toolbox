import { useState, useRef, useEffect, useCallback } from "react";
import { videoRenderer } from "@/lib/videoRenderer";
import { createVideoController } from "@/lib/videoController";
import { cloneBackgroundConfig } from "@/lib/backgroundConfig";
import { thumbnailGenerator } from "@/lib/thumbnailGenerator";
import { getBaseTimelineThumbnailCount } from "@/lib/timelineThumbnailCount";
import {
  BackgroundConfig,
  VideoSegment,
  MousePosition,
  WebcamConfig,
} from "@/types/video";

// ============================================================================
// useVideoPlayback
// ============================================================================
interface UseVideoPlaybackProps {
  segment: VideoSegment | null;
  backgroundConfig: BackgroundConfig;
  webcamConfig?: WebcamConfig;
  mousePositionsRef: { current: MousePosition[] };
  isCropping: boolean;
  interactiveBackgroundPreview?: boolean;
}

export function useVideoPlayback({
  segment,
  backgroundConfig,
  webcamConfig,
  mousePositionsRef,
  isCropping,
  interactiveBackgroundPreview = false,
}: UseVideoPlaybackProps) {
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [isPlaying, setIsPlaying] = useState(false);
  const [isBuffering, setIsBuffering] = useState(false);
  const [isVideoReady, setIsVideoReady] = useState(false);
  const [thumbnails, setThumbnails] = useState<string[]>([]);
  const [currentVideo, setCurrentVideo] = useState<string | null>(null);
  const [currentAudio, setCurrentAudio] = useState<string | null>(null);
  const [currentMicAudio, setCurrentMicAudio] = useState<string | null>(null);
  const [currentWebcamVideo, setCurrentWebcamVideo] = useState<string | null>(
    null,
  );

  const videoRef = useRef<HTMLVideoElement | null>(null);
  const webcamVideoRef = useRef<HTMLVideoElement | null>(null);
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const micAudioRef = useRef<HTMLAudioElement | null>(null);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const tempCanvasRef = useRef<HTMLCanvasElement>(
    document.createElement("canvas"),
  );
  const videoControllerRef = useRef<ReturnType<typeof createVideoController>>();
  const currentVideoRef = useRef<string | null>(null);
  const currentAudioRef = useRef<string | null>(null);
  const currentMicAudioRef = useRef<string | null>(null);
  const currentWebcamVideoRef = useRef<string | null>(null);
  const thumbnailRequestIdRef = useRef(0);
  const thumbnailTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const thumbnailCacheRef = useRef<Map<string, string[]>>(new Map());

  const getRequestedThumbnailCount = useCallback(
    (thumbnailSegment: VideoSegment | null | undefined) => {
      return getBaseTimelineThumbnailCount(thumbnailSegment);
    },
    [],
  );

  // Initialize controller
  useEffect(() => {
    if (!videoRef.current || !canvasRef.current) return;

    videoControllerRef.current = createVideoController({
      videoRef: videoRef.current,
      webcamVideoRef: webcamVideoRef.current || undefined,
      deviceAudioRef: audioRef.current || undefined,
      micAudioRef: micAudioRef.current || undefined,
      canvasRef: canvasRef.current,
      tempCanvasRef: tempCanvasRef.current,
      onTimeUpdate: setCurrentTime,
      onPlayingChange: setIsPlaying,
      onBufferingChange: setIsBuffering,
      onVideoReady: setIsVideoReady,
      onDurationChange: setDuration,
      onError: console.error,
      onMetadataLoaded: () => {
        // Segment update handled in App.tsx via useUndoRedo
      },
    });

    return () => {
      videoControllerRef.current?.destroy();
    };
  }, []);

  const renderFrame = useCallback(() => {
    if (!segment || !videoRef.current || !canvasRef.current) return;
    if (!videoRef.current.paused) return;

    const renderSegment = isCropping
      ? {
          ...segment,
          crop: undefined,
          zoomKeyframes: segment.zoomKeyframes.map((k) => ({
            ...k,
            zoomFactor: 1.0,
            positionX: 0.5,
            positionY: 0.5,
          })),
        }
      : segment;

    const renderBackground = isCropping
      ? {
          ...backgroundConfig,
          scale: 100,
          borderRadius: 0,
          shadow: 0,
          backgroundType: "solid" as const,
          customBackground: undefined,
          cropBottom: 0,
          canvasMode: "auto" as const,
        }
      : cloneBackgroundConfig(backgroundConfig);

    videoRenderer.drawFrame({
      video: videoRef.current,
      webcamVideo: webcamVideoRef.current,
      canvas: canvasRef.current,
      tempCanvas: tempCanvasRef.current,
      segment: renderSegment,
      backgroundConfig: renderBackground,
      webcamConfig,
      mousePositions: mousePositionsRef.current,
      currentTime: videoRef.current.currentTime,
      interactiveBackgroundPreview,
    });
  }, [
    segment,
    backgroundConfig,
    webcamConfig,
    interactiveBackgroundPreview,
    isCropping,
  ]);

  const togglePlayPause = useCallback(() => {
    videoControllerRef.current?.togglePlayPause();
  }, []);

  const seek = useCallback((time: number) => {
    videoControllerRef.current?.seek(time);
  }, []);

  const flushSeek = useCallback(() => {
    videoControllerRef.current?.flushPendingSeek();
  }, []);

  const generateThumbnail = useCallback((): string | undefined => {
    if (!canvasRef.current) return undefined;
    try {
      return canvasRef.current.toDataURL("image/jpeg", 0.5);
    } catch {
      return undefined;
    }
  }, []);

  const getThumbnailCacheKey = useCallback(
    (options?: {
      videoUrl?: string | null;
      filePath?: string;
      segment?: VideoSegment | null;
      thumbnailCount?: number;
    }) => {
      const sourceKey =
        options?.filePath?.trim() ||
        options?.videoUrl?.trim() ||
        currentVideo?.trim() ||
        "";
      const thumbnailSegment = options?.segment ?? segment;
      if (!sourceKey || !thumbnailSegment) return null;
      return JSON.stringify({
        sourceKey,
        thumbnailCount:
          options?.thumbnailCount ??
          getRequestedThumbnailCount(thumbnailSegment),
        trimStart: thumbnailSegment.trimStart,
        trimEnd: thumbnailSegment.trimEnd,
        trimSegments: (thumbnailSegment.trimSegments ?? []).map(
          (trimSegment) => ({
            startTime: trimSegment.startTime,
            endTime: trimSegment.endTime,
          }),
        ),
      });
    },
    [currentVideo, segment],
  );

  const generateThumbnailsForSource = useCallback(
    async (options?: {
      videoUrl?: string | null;
      filePath?: string;
      segment?: VideoSegment | null;
      deferMs?: number;
      thumbnailCount?: number;
    }) => {
      const videoUrl = options?.videoUrl ?? currentVideo;
      const thumbnailSegment = options?.segment ?? segment;
      if (!videoUrl || !thumbnailSegment) return;
      const requestedCount =
        options?.thumbnailCount ?? getRequestedThumbnailCount(thumbnailSegment);
      const requestId = ++thumbnailRequestIdRef.current;
      const cacheKey = getThumbnailCacheKey(options);
      const cachedThumbnails = cacheKey
        ? thumbnailCacheRef.current.get(cacheKey)
        : undefined;
      if (thumbnailTimerRef.current) {
        clearTimeout(thumbnailTimerRef.current);
        thumbnailTimerRef.current = null;
      }
      if (cachedThumbnails && cachedThumbnails.length > 0) {
        setThumbnails(cachedThumbnails);
      }

      const run = async (attempt: number = 0): Promise<void> => {
        try {
          const sourceDuration = Math.max(duration, thumbnailSegment.trimEnd, 0.001);
          const newThumbnails = await thumbnailGenerator.generateSegmentThumbnails(
            videoUrl,
            thumbnailSegment,
            sourceDuration,
            requestedCount,
            {
              width: 240,
              height: 135,
              quality: 0.72,
            },
          );
          if (thumbnailRequestIdRef.current !== requestId) return;
          if (newThumbnails.length === 0) {
            throw new Error("No thumbnails were generated");
          }
          if (cacheKey) {
            thumbnailCacheRef.current.set(cacheKey, newThumbnails);
          }
          setThumbnails(newThumbnails);
        } catch (error) {
          if (thumbnailRequestIdRef.current !== requestId) return;
          if (cachedThumbnails && cachedThumbnails.length > 0) {
            setThumbnails(cachedThumbnails);
            return;
          }
          if (attempt < 1) {
            await new Promise((resolve) => window.setTimeout(resolve, 120));
            if (thumbnailRequestIdRef.current !== requestId) return;
            await run(attempt + 1);
            return;
          }
          const fallbackThumbnail = generateThumbnail();
          if (fallbackThumbnail) {
            const fallbackStrip = Array.from(
              { length: Math.max(1, requestedCount) },
              () => fallbackThumbnail,
            );
            if (cacheKey) {
              thumbnailCacheRef.current.set(cacheKey, fallbackStrip);
            }
            setThumbnails(fallbackStrip);
            return;
          }
          console.error("[Thumbnail] Failed to generate thumbnails", error);
        }
      };

      if ((options?.deferMs ?? 0) > 0) {
        thumbnailTimerRef.current = setTimeout(() => {
          thumbnailTimerRef.current = null;
          void run();
        }, options?.deferMs);
        return;
      }

      await run();
    },
    [
      currentVideo,
      duration,
      generateThumbnail,
      getRequestedThumbnailCount,
      getThumbnailCacheKey,
      segment,
    ],
  );

  const generateThumbnails = useCallback(
    async (filePathOverride?: string) => {
      await generateThumbnailsForSource({ filePath: filePathOverride });
    },
    [generateThumbnailsForSource],
  );

  const invalidateThumbnails = useCallback(() => {
    thumbnailRequestIdRef.current += 1;
    if (thumbnailTimerRef.current) {
      clearTimeout(thumbnailTimerRef.current);
      thumbnailTimerRef.current = null;
    }
    setThumbnails([]);
  }, []);

  useEffect(() => {
    return () => {
      if (thumbnailTimerRef.current) {
        clearTimeout(thumbnailTimerRef.current);
      }
      thumbnailCacheRef.current.clear();
    };
  }, []);

  // Render options sync — apply isCropping overrides so the controller always
  // renders the correct view (e.g. after seeked events, thumbnail generation).
  useEffect(() => {
    if (!segment || !videoControllerRef.current) return;

    const renderSegment = isCropping
      ? {
          ...segment,
          crop: undefined,
          zoomKeyframes: segment.zoomKeyframes.map((k) => ({
            ...k,
            zoomFactor: 1.0,
            positionX: 0.5,
            positionY: 0.5,
          })),
        }
      : segment;

    const renderBackground = isCropping
      ? {
          ...backgroundConfig,
          scale: 100,
          borderRadius: 0,
          shadow: 0,
          backgroundType: "solid" as const,
          customBackground: undefined,
          cropBottom: 0,
          canvasMode: "auto" as const,
        }
      : cloneBackgroundConfig(backgroundConfig);

    videoControllerRef.current.updateRenderOptions({
      segment: renderSegment,
      backgroundConfig: renderBackground,
      webcamConfig,
      mousePositions: mousePositionsRef.current,
      interactiveBackgroundPreview,
    });
  }, [segment, backgroundConfig, webcamConfig, interactiveBackgroundPreview, isCropping]);

  // Render context sync — update the running animation loop's context when
  // segment/backgroundConfig/isCropping change, WITHOUT restarting the loop.
  // VideoController.handlePlay owns startAnimation; the loop self-exits on pause.
  // This eliminates the stop→start thrashing that caused audio play/pause AbortErrors.
  useEffect(() => {
    const video = videoRef.current;
    if (!video || !segment) return;

    const loopSegment = isCropping
      ? {
          ...segment,
          crop: undefined,
          zoomKeyframes: segment.zoomKeyframes.map((k) => ({
            ...k,
            zoomFactor: 1.0,
            positionX: 0.5,
            positionY: 0.5,
          })),
        }
      : segment;

    const loopBackground = isCropping
      ? {
          ...backgroundConfig,
          scale: 100,
          borderRadius: 0,
          shadow: 0,
          backgroundType: "solid" as const,
          customBackground: undefined,
          cropBottom: 0,
          canvasMode: "auto" as const,
        }
      : cloneBackgroundConfig(backgroundConfig);

    // Update context for the animation loop (picked up on next RAF tick)
    videoRenderer.updateRenderContext({
      video,
      webcamVideo: webcamVideoRef.current,
      canvas: canvasRef.current!,
      tempCanvas: tempCanvasRef.current,
      segment: loopSegment,
      backgroundConfig: loopBackground,
      webcamConfig,
      mousePositions: mousePositionsRef.current,
      currentTime: video.currentTime,
      interactiveBackgroundPreview,
    });

    if (video.paused) {
      renderFrame();
    }
  }, [segment, backgroundConfig, webcamConfig, interactiveBackgroundPreview, isCropping]);

  // Cleanup URLs
  useEffect(() => {
    currentVideoRef.current = currentVideo;
  }, [currentVideo]);

  useEffect(() => {
    currentAudioRef.current = currentAudio;
  }, [currentAudio]);

  useEffect(() => {
    currentMicAudioRef.current = currentMicAudio;
  }, [currentMicAudio]);

  useEffect(() => {
    currentWebcamVideoRef.current = currentWebcamVideo;
  }, [currentWebcamVideo]);

  useEffect(() => {
    return () => {
      if (currentVideoRef.current?.startsWith("blob:")) {
        URL.revokeObjectURL(currentVideoRef.current);
      }
      if (currentAudioRef.current?.startsWith("blob:")) {
        URL.revokeObjectURL(currentAudioRef.current);
      }
      if (currentMicAudioRef.current?.startsWith("blob:")) {
        URL.revokeObjectURL(currentMicAudioRef.current);
      }
      if (currentWebcamVideoRef.current?.startsWith("blob:")) {
        URL.revokeObjectURL(currentWebcamVideoRef.current);
      }
    };
  }, []);

  return {
    currentTime,
    setCurrentTime,
    duration,
    setDuration,
    isPlaying,
    isBuffering,
    isVideoReady,
    setIsVideoReady,
    thumbnails,
    setThumbnails,
    currentVideo,
    setCurrentVideo,
    currentAudio,
    setCurrentAudio,
    currentMicAudio,
    setCurrentMicAudio,
    currentWebcamVideo,
    setCurrentWebcamVideo,
    videoRef,
    webcamVideoRef,
    audioRef,
    micAudioRef,
    canvasRef,
    tempCanvasRef,
    videoControllerRef,
    renderFrame,
    togglePlayPause,
    seek,
    flushSeek,
    generateThumbnail,
    generateThumbnails,
    generateThumbnailsForSource,
    invalidateThumbnails,
  };
}
