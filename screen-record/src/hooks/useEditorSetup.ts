import { type MutableRefObject, type RefObject } from "react";
import {
  BackgroundConfig,
  ProjectComposition,
  VideoSegment,
} from "@/types/video";
import type { ZoomKeyframe } from "@/types/video";
import { useCanvasConfig } from "@/hooks/useCanvasConfig";
import { useKeystrokeOverlayEditor } from "@/hooks/useKeystrokeOverlayEditor";
import { usePreviewInteraction } from "@/hooks/usePreviewInteraction";
import type { ActivePanel } from "@/components/sidepanel/index";

export interface UseEditorSetupParams {
  // Shared
  segment: VideoSegment | null;
  setSegment: (s: VideoSegment) => void;
  backgroundConfig: BackgroundConfig;
  setBackgroundConfig: (
    update: BackgroundConfig | ((prev: BackgroundConfig) => BackgroundConfig),
  ) => void;
  composition: ProjectComposition | null;
  activeClipId: string | null | undefined;
  currentTime: number;
  duration: number;
  isCropping: boolean;
  setIsCropping: (value: boolean) => void;
  isPlaying: boolean;
  handleTogglePlayPause: () => void;
  currentVideo: string | null;
  // Refs
  videoRef: MutableRefObject<HTMLVideoElement | null>;
  canvasRef: MutableRefObject<HTMLCanvasElement | null>;
  previewContainerRef: RefObject<HTMLDivElement | null>;
  // useEditorOverlayTools outputs (needed by canvas and preview)
  setZoomFactor: (factor: number) => void;
  setEditingKeyframeId: (id: number | null) => void;
  handleAddKeyframe: (override?: Partial<ZoomKeyframe>) => void;
  // Panel
  activePanel: ActivePanel;
  setActivePanel: (panel: ActivePanel) => void;
  // Batch
  beginBatch: () => void;
  commitBatch: () => void;
}

export function useEditorSetup({
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
}: UseEditorSetupParams) {
  const canvasConfig = useCanvasConfig({
    segment,
    setSegment,
    backgroundConfig,
    setBackgroundConfig,
    composition,
    activeClipId,
    videoRef,
    canvasRef,
    setActivePanel,
    setZoomFactor,
    setEditingKeyframeId,
    isCropping,
    setIsCropping,
    isPlaying,
    handleTogglePlayPause,
  });

  const keystroke = useKeystrokeOverlayEditor({
    segment,
    setSegment,
    currentTime,
    duration,
    canvasRef,
    previewContainerRef,
  });
  const {
    editingKeystrokeSegmentId,
    setEditingKeystrokeSegmentId,
    isKeystrokeOverlaySelected,
    setIsKeystrokeOverlaySelected,
    isKeystrokeResizeHandleHover,
    setIsKeystrokeResizeHandleHover,
    isKeystrokeResizeDragging,
    setIsKeystrokeResizeDragging,
    getKeystrokeTimelineDuration,
    keystrokeOverlayEditFrame,
    handleAddKeystrokeSegment,
    handleDeleteKeystrokeSegment,
    handleToggleKeystrokeMode,
    handleKeystrokeDelayChange,
  } = keystroke;

  const previewInteraction = usePreviewInteraction({
    currentVideo,
    isCropping,
    activePanel,
    setActivePanel,
    isPlaying,
    handleTogglePlayPause,
    handleAddKeyframe,
    beginBatch,
    commitBatch,
    previewContainerRef,
    isKeystrokeResizeDragging,
    isKeystrokeResizeHandleHover,
  });

  return {
    // Canvas config
    ...canvasConfig,
    // Keystroke
    editingKeystrokeSegmentId,
    setEditingKeystrokeSegmentId,
    isKeystrokeOverlaySelected,
    setIsKeystrokeOverlaySelected,
    isKeystrokeResizeHandleHover,
    setIsKeystrokeResizeHandleHover,
    isKeystrokeResizeDragging,
    setIsKeystrokeResizeDragging,
    getKeystrokeTimelineDuration,
    keystrokeOverlayEditFrame,
    handleAddKeystrokeSegment,
    handleDeleteKeystrokeSegment,
    handleToggleKeystrokeMode,
    handleKeystrokeDelayChange,
    // Preview interaction
    ...previewInteraction,
  };
}
