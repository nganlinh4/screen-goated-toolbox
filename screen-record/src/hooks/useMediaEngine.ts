import { useLayoutEffect, useRef } from "react";
import { useVideoPlayback, useRecording } from "@/hooks/useVideoState";
import type { BackgroundConfig, MousePosition, VideoSegment, WebcamConfig } from "@/types/video";

interface UseMediaEngineParams {
  segment: VideoSegment | null;
  backgroundConfig: BackgroundConfig;
  webcamConfig: WebcamConfig;
  isCropping: boolean;
  isCanvasResizeDragging: boolean;
  setSegment: (segment: VideoSegment | null) => void;
}

export function useMediaEngine({
  segment,
  backgroundConfig,
  webcamConfig,
  isCropping,
  isCanvasResizeDragging,
  setSegment,
}: UseMediaEngineParams) {
  // mousePositionsRef is a stable ref shared between useVideoPlayback and the layout effect
  const mousePositionsRef = useRef<MousePosition[]>([]);

  const playback = useVideoPlayback({
    segment,
    backgroundConfig,
    webcamConfig,
    mousePositionsRef,
    isCropping,
    interactiveBackgroundPreview: isCanvasResizeDragging,
  });
  const {
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
