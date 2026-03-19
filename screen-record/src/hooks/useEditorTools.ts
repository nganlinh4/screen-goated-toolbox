import type { MutableRefObject, RefObject } from "react";
import type {
  BackgroundConfig,
  MousePosition,
  ProjectComposition,
  VideoSegment,
} from "@/types/video";
import type { ActivePanel } from "@/components/sidepanel/index";
import { useEditorOverlayTools } from "@/hooks/useEditorOverlayTools";
import { useEditorSetup } from "@/hooks/useEditorSetup";

export interface UseEditorToolsParams {
  // Shared
  segment: VideoSegment | null;
  setSegment: (s: VideoSegment | null) => void;
  currentTime: number;
  duration: number;
  backgroundConfig: BackgroundConfig;
  activePanel: ActivePanel;
  setActivePanel: (panel: ActivePanel) => void;
  videoRef: MutableRefObject<HTMLVideoElement | null>;
  // useEditorOverlayTools-only
  isVideoReady: boolean;
  mousePositions: MousePosition[];
  currentProjectId: string | null;
  loadProjects: () => Promise<void>;
  renderFrame: () => void;
  // useEditorSetup-only
  setBackgroundConfig: (
    update: BackgroundConfig | ((prev: BackgroundConfig) => BackgroundConfig),
  ) => void;
  composition: ProjectComposition | null;
  activeClipId: string | null | undefined;
  isCropping: boolean;
  setIsCropping: (value: boolean) => void;
  isPlaying: boolean;
  handleTogglePlayPause: () => void;
  currentVideo: string | null;
  canvasRef: MutableRefObject<HTMLCanvasElement | null>;
  previewContainerRef: RefObject<HTMLDivElement | null>;
  beginBatch: () => void;
  commitBatch: () => void;
}

export function useEditorTools({
  segment,
  setSegment,
  currentTime,
  duration,
  backgroundConfig,
  activePanel,
  setActivePanel,
  videoRef,
  isVideoReady,
  mousePositions,
  currentProjectId,
  loadProjects,
  renderFrame,
  setBackgroundConfig,
  composition,
  activeClipId,
  isCropping,
  setIsCropping,
  isPlaying,
  handleTogglePlayPause,
  currentVideo,
  canvasRef,
  previewContainerRef,
  beginBatch,
  commitBatch,
}: UseEditorToolsParams) {
  const overlayTools = useEditorOverlayTools({
    segment,
    setSegment,
    currentTime,
    duration,
    isVideoReady,
    videoRef,
    mousePositions,
    backgroundConfig,
    currentProjectId,
    loadProjects,
    activePanel,
    setActivePanel,
    renderFrame,
  });
  const {
    setZoomFactor,
    setEditingKeyframeId,
    handleAddKeyframe,
  } = overlayTools;

  const editorSetup = useEditorSetup({
    segment,
    setSegment,
    backgroundConfig,
    setBackgroundConfig,
    composition,
    activeClipId,
    currentTime,
    duration,
    isCropping,
    setIsCropping,
    isPlaying,
    handleTogglePlayPause,
    currentVideo,
    videoRef,
    canvasRef,
    previewContainerRef,
    setZoomFactor,
    setEditingKeyframeId,
    handleAddKeyframe,
    activePanel,
    setActivePanel,
    beginBatch,
    commitBatch,
  });

  return {
    ...overlayTools,
    ...editorSetup,
  };
}
