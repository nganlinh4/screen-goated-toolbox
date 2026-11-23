import { useCallback, useLayoutEffect, useMemo, useRef, type MutableRefObject } from "react";
import { useVideoPlayback, useRecording } from "@/hooks/useVideoState";
import type { BackgroundConfig, MousePosition, ProjectComposition, VideoSegment, WebcamConfig } from "@/types/video";
import { getCompositionClipRenderSegment } from '@/lib/compositionSubtitles';

interface UseMediaEngineParams {
  segment: VideoSegment | null;
  composition?: ProjectComposition | null;
  backgroundConfig: BackgroundConfig;
  webcamConfig: WebcamConfig;
  isCropping: boolean;
  isCanvasResizeDragging: boolean;
  isTimelineOnly: boolean;
  setSegment: (segment: VideoSegment | null) => void;
}

export function useMediaEngine({
  segment,
  composition,
  backgroundConfig,
  webcamConfig,
  isCropping,
  isCanvasResizeDragging,
  isTimelineOnly,
  setSegment,
}: UseMediaEngineParams) {
  // mousePositionsRef is a stable ref shared between useVideoPlayback and the layout effect
  const mousePositionsRef = useRef<MousePosition[]>([]);
  const subtitleClipOwnerRef = useRef<MutableRefObject<string | null> | null>(null);
  const bindSubtitleClipOwner = useCallback((owner: MutableRefObject<string | null>) => {
    subtitleClipOwnerRef.current = owner;
  }, []);
  const subtitleClipId = subtitleClipOwnerRef.current?.current ?? null;
  const renderSegment = useMemo(() => segment
    ? getCompositionClipRenderSegment(composition, subtitleClipId, segment) : null,
  [composition, segment, subtitleClipId]);

  const playback = useVideoPlayback({
    segment: renderSegment,
    backgroundConfig,
    webcamConfig,
    mousePositionsRef,
    isCropping,
    isTimelineOnly,
    interactiveBackgroundPreview: isCanvasResizeDragging,
  });
  const {
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
    togglePlayPause: togglePlayback,
    seek,
    flushSeek,
    generateThumbnail,
    generateThumbnailsForSource,
    invalidateThumbnails,
  } = playback;

  const recording = useRecording({
    videoControllerRef,
    videoRef,
    canvasRef,
    tempCanvasRef,
    backgroundConfig,
    setSegment,
    setCurrentVideo,
    setCurrentAudio,
    setCurrentMicAudio,
    setCurrentWebcamVideo,
    setIsVideoReady,
    setThumbnails,
    invalidateThumbnails,
    setDuration,
    setCurrentTime,
    generateThumbnailsForSource,
    generateThumbnail,
    renderFrame,
    currentVideo,
    currentAudio,
    currentMicAudio,
    currentWebcamVideo,
  });
  const {
    isRecording,
    recordingDuration,
    isLoadingVideo,
    loadingProgress,
    mousePositions,
    setMousePositions,
    audioFilePath,
    micAudioFilePath,
    webcamVideoFilePath,
    videoFilePath,
    videoFilePathOwnerUrl,
    error,
    setError,
    startNewRecording,
    handleStopRecording,
  } = recording;

  // Sync mouse positions to ref before paint so useVideoPlayback always reads
  // the latest positions without causing stale-closure bugs in Concurrent Mode.
  useLayoutEffect(() => {
    mousePositionsRef.current = mousePositions;
  }, [mousePositions]);

  return {
    // Playback
    currentTime,
    setCurrentTime,
    duration,
    setDuration,
    isPlaying,
    setIsPlaying,
    isBuffering,
    isVideoReady,
    thumbnails,
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
    bindSubtitleClipOwner,
    togglePlayback,
    seek,
    flushSeek,
    generateThumbnail,
    generateThumbnailsForSource,
    invalidateThumbnails,
    setThumbnails,
    // Recording
    isRecording,
    recordingDuration,
    isLoadingVideo,
    loadingProgress,
    mousePositions,
    setMousePositions,
    audioFilePath,
    micAudioFilePath,
    webcamVideoFilePath,
    videoFilePath,
    videoFilePathOwnerUrl,
    error,
    setError,
    startNewRecording,
    handleStopRecording,
  };
}
