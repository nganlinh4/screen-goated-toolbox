import { type MutableRefObject, type RefObject } from "react";
import {
  BackgroundConfig,
  MousePosition,
  Project,
  ProjectComposition,
  RecordingMode,
  VideoSegment,
  WebcamConfig,
} from "@/types/video";
import { useClipMediaCache } from "@/hooks/useClipMediaCache";
import { useProjectInteractionShield } from "@/hooks/useProjectInteractionShield";
import { useSequenceComposition, type PersistOptions } from "@/hooks/useSequenceComposition";
import type { VideoController } from "@/lib/videoController";

export interface UseCompositionPipelineParams {
  // Shared state
  composition: ProjectComposition | null;
  setComposition: (c: ProjectComposition | null | ((prev: ProjectComposition | null) => ProjectComposition | null)) => void;
  currentProjectData: Project | null;
  setCurrentProjectData: (p: Project | null) => void;
  segment: VideoSegment | null;
  backgroundConfig: BackgroundConfig;
  mousePositions: MousePosition[];
  duration: number;
  currentRawVideoPath: string;
  currentRecordingMode: RecordingMode;
  currentProjectId: string | null;
  // Video/audio state
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
  // Refs
  videoControllerRef: MutableRefObject<VideoController | undefined>;
  webcamVideoRef: RefObject<HTMLVideoElement | null>;
  canvasRef: RefObject<HTMLCanvasElement | null>;
  tempCanvasRef: RefObject<HTMLCanvasElement | null>;
  previewContainerRef: MutableRefObject<HTMLDivElement | null>;
  isProjectTransitionRef: MutableRefObject<boolean>;
  persistRef: MutableRefObject<((opts?: PersistOptions) => Promise<void>) | null>;
  // Config setters
  applyLoadedBackgroundConfig: (config: BackgroundConfig) => void;
  setWebcamConfig: (config: WebcamConfig) => void;
  setMousePositions: (positions: MousePosition[]) => void;
  setCurrentRecordingMode: (mode: RecordingMode) => void;
  handleProjectRawVideoPathChange: (path: string) => void;
  setCurrentRawMicAudioPath: (path: string) => void;
  setCurrentRawWebcamVideoPath: (path: string) => void;
  // Projects dialog (for useProjectInteractionShield and useSequenceComposition)
  showProjectsDialog: boolean;
  setShowProjectsDialog: (show: boolean) => void;
  // Playback
  seek: (time: number) => void;
  isPlaying: boolean;
  currentTime: number;
  togglePlayback: () => void;
}

export function useCompositionPipeline({
  composition,
  setComposition,
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
  setWebcamConfig,
  setMousePositions,
  setCurrentRecordingMode,
  handleProjectRawVideoPathChange,
  setCurrentRawMicAudioPath,
  setCurrentRawWebcamVideoPath,
  showProjectsDialog,
  setShowProjectsDialog,
  seek,
  isPlaying,
  currentTime,
  togglePlayback,
}: UseCompositionPipelineParams) {
  const clipMediaCache = useClipMediaCache({
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
  });
  const {
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
    clearClipMediaCaches,
    primePreloadSlot,
    loadClipMediaIntoEditor,
    resolveClipExportSourcePath,
    resolveClipExportMicAudioPath,
    resolveClipExportWebcamPath,
  } = clipMediaCache;

  const {
    isProjectInteractionShieldVisible,
    setIsProjectInteractionShieldVisible,
    isProjectTransitionRef: _,
    projectInteractionShieldReleaseRef,
    projectInteractionBlockCleanupRef,
    beginProjectInteractionShield,
    abortEditorInteractions,
    armProjectInteractionShieldRelease,
  } = useProjectInteractionShield({
    showProjectsDialog,
    previewContainerRef,
    isProjectTransitionRef,
  });

  const sequenceComposition = useSequenceComposition({
    currentProjectId,
    composition,
    setComposition,
    currentProjectData,
    setCurrentProjectData,
    backgroundConfig,
    segment,
    mousePositions,
    duration,
    currentRawVideoPath,
    currentRecordingMode,
    loadedClipId,
    isSwitchingCompositionClipRef,
    isProjectTransitionRef,
    clipLoadRequestSeqRef,
    loadClipMediaIntoEditor,
    clearClipMediaCaches,
    clipAssetCacheRef,
    clipUrlCacheRef,
    clipExportSourcePathCacheRef,
    clipExportMicAudioPathCacheRef,
    clipExportWebcamPathCacheRef,
    preloadedSlotClipIdsRef,
    primePreloadSlot,
    persistRef,
    seek,
    videoControllerRef,
    isPlaying,
    currentTime,
    togglePlayback,
    setShowProjectsDialog,
  });

  return {
    // Clip media cache outputs
    loadedClipId,
    setLoadedClipId,
    isSwitchingCompositionClipRef,
    clipExportSourcePathCacheRef,
    clipExportMicAudioPathCacheRef,
    clipExportWebcamPathCacheRef,
    clipLoadRequestSeqRef,
    previousPreloadVideoRef,
    previousPreloadAudioRef,
    nextPreloadVideoRef,
    nextPreloadAudioRef,
    loadClipAssets,
    clearClipMediaCaches,
    loadClipMediaIntoEditor,
    resolveClipExportSourcePath,
    resolveClipExportMicAudioPath,
    resolveClipExportWebcamPath,
    // Interaction shield outputs
    isProjectInteractionShieldVisible,
    setIsProjectInteractionShieldVisible,
    projectInteractionShieldReleaseRef,
    projectInteractionBlockCleanupRef,
    beginProjectInteractionShield,
    abortEditorInteractions,
    armProjectInteractionShieldRelease,
    // Sequence composition outputs
    ...sequenceComposition,
  };
}
