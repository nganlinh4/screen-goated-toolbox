import { useCallback, useState } from "react";
import { useAudioDownload } from "@/hooks/useAudioDownload";
import {
  useCompositionPipeline,
  type UseCompositionPipelineParams,
} from "@/hooks/useCompositionPipeline";
import { useDebugEffects } from "@/hooks/useDebugEffects";
import { useTimelineAdaptiveThumbnails } from "@/hooks/useTimelineAdaptiveThumbnails";
import { useExport } from "@/hooks/useVideoState";
import type {
  BackgroundConfig,
  MousePosition,
  MutableRefObject,
  Project,
  ProjectComposition,
  RecordingMode,
  RefObject,
  VideoSegment,
  WebcamConfig,
} from "@/hooks/appControllerTypes";

type PipelineArg<K extends keyof UseCompositionPipelineParams> =
  UseCompositionPipelineParams[K];

export interface AppCompositionExportControllerArgs {
  applyLoadedBackgroundConfig: PipelineArg<"applyLoadedBackgroundConfig">;
  audioFilePath: string;
  audioRef: RefObject<HTMLAudioElement | null>;
  backgroundConfig: BackgroundConfig;
  backgroundMutationMetaRef: MutableRefObject<{ at: number; stack: string[] } | null>;
  canvasRef: RefObject<HTMLCanvasElement | null>;
  composition: ProjectComposition | null;
  currentAudio: string | null;
  currentMicAudio: string | null;
  currentProjectData: Project | null;
  currentProjectDataRef: MutableRefObject<Project | null>;
  currentProjectId: string | null;
  currentRawMicAudioPath: string;
  currentRawVideoPath: string;
  currentRawWebcamVideoPath: string;
  currentRecordingMode: RecordingMode;
  currentTime: number;
  currentVideo: string | null;
  currentWebcamVideo: string | null;
  duration: number;
  generateThumbnailsForSource: (options?: {
    videoUrl?: string | null;
    filePath?: string;
    segment?: VideoSegment | null;
    deferMs?: number;
    thumbnailCount?: number;
  }) => Promise<void>;
  handleProjectRawVideoPathChange: (path: string) => void;
  invalidateThumbnails: () => void;
  isBatching: boolean;
  isCropping: boolean;
  isLoadingVideo: boolean;
  isPlaying: boolean;
  isProjectTransitionRef: MutableRefObject<boolean>;
  isRecording: boolean;
  isVideoReady: boolean;
  lastRawSavedPath: string;
  micAudioFilePath: string;
  micAudioRef: RefObject<HTMLAudioElement | null>;
  mousePositions: MousePosition[];
  persistRef: PipelineArg<"persistRef">;
  previewContainerRef: MutableRefObject<HTMLDivElement | null>;
  rawSetCurrentRawMicAudioPath: (path: string) => void;
  rawSetCurrentRawWebcamVideoPath: (path: string) => void;
  rawSetCurrentRecordingMode: (mode: RecordingMode) => void;
  rawSetWebcamConfig: (config: WebcamConfig) => void;
  segment: VideoSegment | null;
  seek: (time: number) => void;
  setComposition: PipelineArg<"setComposition">;
  setCompositionSilently: PipelineArg<"setCompositionSilently">;
  setCurrentAudio: (url: string | null) => void;
  setCurrentMicAudio: (url: string | null) => void;
  setCurrentProjectData: (p: Project | null) => void;
  setCurrentVideo: (url: string | null) => void;
  setCurrentWebcamVideo: (url: string | null) => void;
  setMousePositions: (positions: MousePosition[]) => void;
  setPreviewDuration: (duration: number) => void;
  setSegment: PipelineArg<"setSegment">;
  setShowProjectsDialog: (show: boolean) => void;
  setThumbnails: (thumbnails: string[]) => void;
  showProjectsDialog: boolean;
  tempCanvasRef: RefObject<HTMLCanvasElement | null>;
  thumbnails: string[];
  timelineCanvasWidthPx: number;
  togglePlayback: () => void;
  videoControllerRef: PipelineArg<"videoControllerRef">;
  videoFilePath: string;
  videoFilePathOwnerUrl: string;
  videoRef: RefObject<HTMLVideoElement | null>;
  webcamConfig: WebcamConfig;
  webcamVideoFilePath: string;
  webcamVideoRef: RefObject<HTMLVideoElement | null>;
}

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
    // App's tempCanvasRef is `RefObject<HTMLCanvasElement | null>`; useExport's param
    // is the non-null variant. Narrowing cast is typing-only (matches prior `any` flow).
    tempCanvasRef: tempCanvasRef as RefObject<HTMLCanvasElement>,
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
