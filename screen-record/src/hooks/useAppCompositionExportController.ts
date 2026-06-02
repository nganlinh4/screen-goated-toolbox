import { useCallback, useState } from "react";
import { useAudioDownload } from "@/hooks/useAudioDownload";
import { useCompositionPipeline } from "@/hooks/useCompositionPipeline";
import { useDebugEffects } from "@/hooks/useDebugEffects";
import { useTimelineAdaptiveThumbnails } from "@/hooks/useTimelineAdaptiveThumbnails";
import { useExport } from "@/hooks/useVideoState";

type AppCompositionExportControllerArgs = Record<string, any>;

export function useAppCompositionExportController(args: AppCompositionExportControllerArgs) {
  const {
    applyLoadedBackgroundConfig,
    audioFilePath,
    audioRef,
    backgroundConfig,
    backgroundMutationMetaRef,
    canvasRef,
    composition,
    currentAudio,
    currentMicAudio,
    currentProjectData,
    currentProjectDataRef,
    currentProjectId,
    currentRawMicAudioPath,
    currentRawVideoPath,
    currentRawWebcamVideoPath,
    currentRecordingMode,
    currentTime,
    currentVideo,
    currentWebcamVideo,
    duration,
    generateThumbnailsForSource,
    handleProjectRawVideoPathChange,
    invalidateThumbnails,
    isBatching,
    isCropping,
    isLoadingVideo,
    isPlaying,
    isProjectTransitionRef,
    isRecording,
    isVideoReady,
    lastRawSavedPath,
    micAudioFilePath,
    micAudioRef,
    mousePositions,
    persistRef,
    previewContainerRef,
    rawSetCurrentRawMicAudioPath,
    rawSetCurrentRawWebcamVideoPath,
    rawSetCurrentRecordingMode,
    rawSetWebcamConfig,
    segment,
    seek,
    setComposition,
    setCompositionSilently,
    setCurrentAudio,
    setCurrentMicAudio,
    setCurrentProjectData,
    setCurrentVideo,
    setCurrentWebcamVideo,
    setMousePositions,
    setPreviewDuration,
    setSegment,
    setShowProjectsDialog,
    setThumbnails,
    showProjectsDialog,
    tempCanvasRef,
    thumbnails,
    timelineCanvasWidthPx,
    togglePlayback,
    videoControllerRef,
    videoFilePath,
    videoFilePathOwnerUrl,
    videoRef,
    webcamConfig,
    webcamVideoFilePath,
    webcamVideoRef,
  } = args;

  const compositionPipeline = useCompositionPipeline({
    composition,
    setComposition,
    setCompositionSilently,
    currentProjectData,
    setCurrentProjectData,
    segment,
    backgroundConfig,
    mousePositions,
    duration,
    currentRawVideoPath,
    currentRecordingMode,
    currentProjectId,
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
    previewContainerRef,
    isProjectTransitionRef,
    persistRef,
    applyLoadedBackgroundConfig,
    setWebcamConfig: rawSetWebcamConfig,
    setMousePositions,
    setCurrentRecordingMode: rawSetCurrentRecordingMode,
    handleProjectRawVideoPathChange,
    setCurrentRawMicAudioPath: rawSetCurrentRawMicAudioPath,
    setCurrentRawWebcamVideoPath: rawSetCurrentRawWebcamVideoPath,
    showProjectsDialog,
    setShowProjectsDialog,
    seek,
    isPlaying,
    currentTime,
    togglePlayback,
  });

  useTimelineAdaptiveThumbnails({
    timelineCanvasWidthPx,
    segment,
    currentVideo,
    currentRawVideoPath,
    thumbnailsLength: thumbnails.length,
    isPlaying,
    generateThumbnailsForSource,
  });

  useDebugEffects({
    backgroundConfig,
    isProjectTransitionRef,
    isSwitchingCompositionClipRef: compositionPipeline.isSwitchingCompositionClipRef,
    isCropping,
    currentProjectId,
    showProjectsDialog,
    backgroundMutationMetaRef,
    currentTime,
    currentVideo,
    isRecording,
    isLoadingVideo,
    isPlaying,
    isVideoReady,
    hasSequenceChain: compositionPipeline.hasSequenceChain,
    loadedClipId: compositionPipeline.loadedClipId,
    selectedClipId: compositionPipeline.selectedClipId,
  });

  const handleTogglePlayPause = useCallback(() => {
    compositionPipeline.handleTogglePlayPause();
  }, [compositionPipeline]);

  // FPS of the most-recent recording (set on stop, cleared when a different project loads).
  const [lastCaptureFps, setLastCaptureFps] = useState<number | null>(null);

  const exportHook = useExport({
    videoRef,
    webcamVideoRef,
    canvasRef,
    tempCanvasRef,
    audioRef,
    micAudioRef,
    segment,
    backgroundConfig,
    webcamConfig,
    isRecording,
    isBatchEditing: isBatching,
    mousePositions,
    audioFilePath,
    micAudioFilePath: micAudioFilePath || currentRawMicAudioPath,
    webcamVideoFilePath: webcamVideoFilePath || currentRawWebcamVideoPath,
    videoFilePath,
    videoFilePathOwnerUrl,
    rawVideoPath: currentRawVideoPath,
    savedRawVideoPath: lastRawSavedPath,
    currentVideo,
    lastCaptureFps,
    composition,
    getLatestComposition: () => currentProjectDataRef.current?.composition ?? composition,
    currentProjectId,
    resolveClipExportSourcePath: compositionPipeline.resolveClipExportSourcePath,
    resolveClipExportMicAudioPath: compositionPipeline.resolveClipExportMicAudioPath,
    resolveClipExportWebcamPath: compositionPipeline.resolveClipExportWebcamPath,
  });

  const resolveCurrentExportSourcePath = useCallback((): string => {
    const directRecordingPath =
      currentVideo === videoFilePathOwnerUrl
        ? videoFilePath
        : "";
    return (
      directRecordingPath ||
      currentRawVideoPath ||
      lastRawSavedPath ||
      ""
    ).trim();
  }, [
    currentRawVideoPath,
    currentVideo,
    lastRawSavedPath,
    videoFilePath,
    videoFilePathOwnerUrl,
  ]);

  const audioDownloadHook = useAudioDownload({
    videoRef,
    segment,
    sourceVideoPath: resolveCurrentExportSourcePath(),
    micAudioPath: micAudioFilePath || currentRawMicAudioPath,
    composition,
    getLatestComposition: () => currentProjectDataRef.current?.composition ?? composition,
    resolveClipExportSourcePath: compositionPipeline.resolveClipExportSourcePath,
    resolveClipExportMicAudioPath: compositionPipeline.resolveClipExportMicAudioPath,
  });

  return {
    ...compositionPipeline,
    audioDownloadHook,
    exportHook,
    handleTogglePlayPause,
    setLastCaptureFps,
  };
}
