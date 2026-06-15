import { useState, useRef, useEffect, useCallback, useMemo } from "react";
import { videoRenderer } from "@/lib/videoRenderer";
import { createVideoController } from "@/lib/videoController";
import { cloneBackgroundConfig } from "@/lib/backgroundConfig";
import { normalizeSubtitleTrackState } from "@/lib/subtitleTracks";
import { thumbnailGenerator } from "@/lib/thumbnailGenerator";
import { getBaseTimelineThumbnailCount } from "@/lib/timelineThumbnailCount";
import { getSpeedAtTime } from "@/lib/videoExporter";
import {
  buildPlaybackStructureSignature,
  getPlaybackRenderBackground,
  getPlaybackRenderSegment,
} from "./videoPlaybackRenderState";
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
  isTimelineOnly: boolean;
  interactiveBackgroundPreview?: boolean;
}

export function useVideoPlayback({
  segment,
  backgroundConfig,
  webcamConfig,
  mousePositionsRef,
  isCropping,
  isTimelineOnly,
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
  const videoControllerRef = useRef<ReturnType<typeof createVideoController> | undefined>(undefined);
  const currentVideoRef = useRef<string | null>(null);
  const currentAudioRef = useRef<string | null>(null);
  const currentMicAudioRef = useRef<string | null>(null);
  const currentWebcamVideoRef = useRef<string | null>(null);
  const thumbnailRequestIdRef = useRef(0);
  const thumbnailTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const thumbnailCacheRef = useRef<Map<string, string[]>>(new Map());
  const lastPlaybackStructureSignatureRef = useRef("");
  const lastLoopStructureSignatureRef = useRef("");
  const lastSubtitleRenderSyncAtRef = useRef(0);
  const currentTimeRef = useRef(0);
  const timelineOnlyPlaybackRef = useRef<{ raf: number | null; lastTick: number }>({
    raf: null,
    lastTick: 0,
  });
  const normalizedSegment = useMemo(
    () => (segment ? normalizeSubtitleTrackState(segment) : null),
    [segment],
  );

  const getRequestedThumbnailCount = useCallback(
    (thumbnailSegment: VideoSegment | null | undefined) => {
      return getBaseTimelineThumbnailCount(thumbnailSegment);
    },
    [],
  );

  // Initialize controller
  useEffect(() => {
    if (!videoRef.current || !canvasRef.current) return;

    const controller = createVideoController({
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
    videoControllerRef.current = controller;

    return () => {
      controller.destroy();
      if (videoControllerRef.current === controller) {
        videoControllerRef.current = undefined;
      }
    };
  }, []);

  const renderTimelineOnlyFrame = useCallback((time: number) => {
    if (!normalizedSegment || !videoRef.current || !canvasRef.current) return;
    void videoRenderer.drawFrame({
      video: videoRef.current,
      webcamVideo: webcamVideoRef.current,
      canvas: canvasRef.current,
      tempCanvas: tempCanvasRef.current,
      segment: { ...normalizedSegment, mediaMode: "timelineOnly" },
      backgroundConfig: cloneBackgroundConfig(backgroundConfig),
      webcamConfig,
      mousePositions: mousePositionsRef.current,
      currentTime: time,
      interactiveBackgroundPreview,
    });
  }, [
    backgroundConfig,
    interactiveBackgroundPreview,
    mousePositionsRef,
    normalizedSegment,
    webcamConfig,
  ]);

  const renderFrame = useCallback(() => {
    if (!normalizedSegment || !videoRef.current || !canvasRef.current) return;
    if (isTimelineOnly) {
      renderTimelineOnlyFrame(currentTimeRef.current);
      return;
    }
    if (!videoRef.current.paused) return;

    videoRenderer.drawFrame({
      video: videoRef.current,
      webcamVideo: webcamVideoRef.current,
      canvas: canvasRef.current,
      tempCanvas: tempCanvasRef.current,
      segment: getPlaybackRenderSegment(normalizedSegment, isCropping),
      backgroundConfig: getPlaybackRenderBackground(backgroundConfig, isCropping),
      webcamConfig,
      mousePositions: mousePositionsRef.current,
      currentTime: videoRef.current.currentTime,
      interactiveBackgroundPreview,
    });
  }, [
    normalizedSegment,
    backgroundConfig,
    webcamConfig,
    interactiveBackgroundPreview,
    isCropping,
    isTimelineOnly,
    renderTimelineOnlyFrame,
  ]);

  const togglePlayPause = useCallback(() => {
    if (isTimelineOnly) {
      setIsPlaying((playing) => !playing);
      return;
    }
    videoControllerRef.current?.togglePlayPause();
  }, [isTimelineOnly]);

  const seek = useCallback((time: number) => {
    if (isTimelineOnly) {
      const safeDuration = Math.max(duration, normalizedSegment?.trimEnd ?? 0, 0);
      const clamped = Math.max(0, Math.min(time, safeDuration));
      currentTimeRef.current = clamped;
      setCurrentTime(clamped);
      renderTimelineOnlyFrame(clamped);
      return;
    }
    videoControllerRef.current?.seek(time);
  }, [duration, isTimelineOnly, normalizedSegment?.trimEnd, renderTimelineOnlyFrame]);

  const flushSeek = useCallback(() => {
    if (isTimelineOnly) return;
    videoControllerRef.current?.flushPendingSeek();
  }, [isTimelineOnly]);

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
              filePath: options?.filePath?.trim() || undefined,
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
      if (timelineOnlyPlaybackRef.current.raf !== null) {
        cancelAnimationFrame(timelineOnlyPlaybackRef.current.raf);
        timelineOnlyPlaybackRef.current.raf = null;
      }
      thumbnailCacheRef.current.clear();
    };
  }, []);

  useEffect(() => {
    currentTimeRef.current = currentTime;
  }, [currentTime]);

  useEffect(() => {
    if (!isTimelineOnly || !normalizedSegment) return;
    const safeDuration = Math.max(normalizedSegment.trimEnd, duration, 1);
    if (duration !== safeDuration) setDuration(safeDuration);
    setIsVideoReady(true);
    setCurrentVideo(null);
    setThumbnails([]);
    if (currentTimeRef.current > safeDuration) {
      currentTimeRef.current = safeDuration;
      setCurrentTime(safeDuration);
    }
    renderTimelineOnlyFrame(currentTimeRef.current);
  }, [duration, isTimelineOnly, renderTimelineOnlyFrame, normalizedSegment]);

  useEffect(() => {
    if (!isTimelineOnly) return;
    if (timelineOnlyPlaybackRef.current.raf !== null) {
      cancelAnimationFrame(timelineOnlyPlaybackRef.current.raf);
      timelineOnlyPlaybackRef.current.raf = null;
    }
    if (!isPlaying || !normalizedSegment) return;

    timelineOnlyPlaybackRef.current.lastTick = performance.now();
    const tick = (now: number) => {
      const playback = timelineOnlyPlaybackRef.current;
      const dt = Math.max(0, (now - playback.lastTick) / 1000);
      playback.lastTick = now;
      const safeDuration = Math.max(normalizedSegment.trimEnd, duration, 1);
      const speed = Math.max(
        0.0625,
        getSpeedAtTime(currentTimeRef.current, normalizedSegment.speedPoints ?? []),
      );
      const nextTime = Math.min(safeDuration, currentTimeRef.current + dt * speed);
      currentTimeRef.current = nextTime;
      setCurrentTime(nextTime);
      renderTimelineOnlyFrame(nextTime);
      if (nextTime >= safeDuration) {
        setIsPlaying(false);
        playback.raf = null;
        return;
      }
      playback.raf = requestAnimationFrame(tick);
    };
    timelineOnlyPlaybackRef.current.raf = requestAnimationFrame(tick);

    return () => {
      if (timelineOnlyPlaybackRef.current.raf !== null) {
        cancelAnimationFrame(timelineOnlyPlaybackRef.current.raf);
        timelineOnlyPlaybackRef.current.raf = null;
      }
    };
  }, [duration, isPlaying, isTimelineOnly, normalizedSegment, renderTimelineOnlyFrame]);

  // Render options sync — apply isCropping overrides so the controller always
  // renders the correct view (e.g. after seeked events, thumbnail generation).
  useEffect(() => {
    if (isTimelineOnly) {
      renderFrame();
      return;
    }
    if (!normalizedSegment || !videoControllerRef.current) return;
    const video = videoRef.current;
    const nextStructureSignature = buildPlaybackStructureSignature(normalizedSegment);
    const isSubtitleOnlyChange =
      lastPlaybackStructureSignatureRef.current !== "" &&
      lastPlaybackStructureSignatureRef.current === nextStructureSignature;
    lastPlaybackStructureSignatureRef.current = nextStructureSignature;

    if (isSubtitleOnlyChange && video && !video.paused) {
      return;
    }

    videoControllerRef.current.updateRenderOptions({
      segment: getPlaybackRenderSegment(normalizedSegment, isCropping),
      backgroundConfig: getPlaybackRenderBackground(backgroundConfig, isCropping),
      webcamConfig,
      mousePositions: mousePositionsRef.current,
      interactiveBackgroundPreview,
    });
  }, [
    normalizedSegment,
    backgroundConfig,
    webcamConfig,
    interactiveBackgroundPreview,
    isCropping,
    isTimelineOnly,
    renderFrame,
  ]);

  // Render context sync — update the running animation loop's context when
  // segment/backgroundConfig/isCropping change, WITHOUT restarting the loop.
  // VideoController.handlePlay owns startAnimation; the loop self-exits on pause.
  // This eliminates the stop→start thrashing that caused audio play/pause AbortErrors.
  useEffect(() => {
    if (isTimelineOnly) {
      renderFrame();
      return;
    }
    const video = videoRef.current;
    if (!video || !normalizedSegment) return;
    const nextStructureSignature = buildPlaybackStructureSignature(normalizedSegment);
    const isSubtitleOnlyChange =
      lastLoopStructureSignatureRef.current !== "" &&
      lastLoopStructureSignatureRef.current === nextStructureSignature;
    lastLoopStructureSignatureRef.current = nextStructureSignature;

    if (isSubtitleOnlyChange && !video.paused) {
      const now = performance.now();
      if (now - lastSubtitleRenderSyncAtRef.current < 1500) {
        return;
      }
      lastSubtitleRenderSyncAtRef.current = now;
    }

    // Update context for the animation loop (picked up on next RAF tick)
    videoRenderer.updateRenderContext({
      video,
      webcamVideo: webcamVideoRef.current,
      canvas: canvasRef.current!,
      tempCanvas: tempCanvasRef.current,
      segment: getPlaybackRenderSegment(normalizedSegment, isCropping),
      backgroundConfig: getPlaybackRenderBackground(backgroundConfig, isCropping),
      webcamConfig,
      mousePositions: mousePositionsRef.current,
      currentTime: video.currentTime,
      interactiveBackgroundPreview,
    });

    if (video.paused) {
      renderFrame();
    }
  }, [
    normalizedSegment,
    backgroundConfig,
    webcamConfig,
    interactiveBackgroundPreview,
    isCropping,
    renderFrame,
    isTimelineOnly,
  ]);

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
    setIsPlaying,
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
