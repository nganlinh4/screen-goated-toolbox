import type { RefObject } from "react";
import {
  useZoomKeyframes,
  useTextOverlays,
  useAutoZoom,
  useCursorHiding,
} from "@/hooks/useVideoState";
import type { BackgroundConfig, MousePosition, VideoSegment } from "@/types/video";
import type { ActivePanel } from "@/components/sidepanel/index";

export interface UseEditorOverlayToolsParams {
  segment: VideoSegment | null;
  setSegment: (s: VideoSegment | null) => void;
  currentTime: number;
  duration: number;
  isVideoReady: boolean;
  videoRef: RefObject<HTMLVideoElement | null>;
  mousePositions: MousePosition[];
  backgroundConfig: BackgroundConfig;
  currentProjectId: string | null;
  loadProjects: () => Promise<void>;
  activePanel: ActivePanel;
  setActivePanel: (panel: ActivePanel) => void;
  renderFrame: () => void;
}

export function useEditorOverlayTools({
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
}: UseEditorOverlayToolsParams) {
  // Zoom keyframes
  const zoomKeyframes = useZoomKeyframes({
    segment,
    setSegment,
    videoRef,
    currentTime,
    isVideoReady,
    renderFrame,
    activePanel,
    setActivePanel,
  });
  const {
    editingKeyframeId,
    setEditingKeyframeId,
    zoomFactor,
    setZoomFactor,
    handleAddKeyframe,
    handleDeleteKeyframe,
    throttledUpdateZoom,
  } = zoomKeyframes;

  // Text overlays
  const textOverlays = useTextOverlays({
    segment,
    setSegment,
    currentTime,
    duration,
    setActivePanel,
  });
  const {
    editingTextId,
    setEditingTextId,
    handleAddText,
    handleDeleteText,
    handleTextDragMove,
  } = textOverlays;

  // Auto zoom
  const { handleAutoZoom } = useAutoZoom({
    segment,
    setSegment,
    videoRef,
    mousePositions,
    duration,
    currentProjectId,
    backgroundConfig,
    loadProjects,
    setActivePanel,
  });

  // Cursor hiding
  const cursorHiding = useCursorHiding({
    segment,
    setSegment,
    mousePositions,
    currentTime,
    duration,
    videoRef,
    backgroundConfig,
  });
  const {
    editingPointerId,
    setEditingPointerId,
    handleSmartPointerHiding,
    handleAddPointerSegment,
    handleDeletePointerSegment,
  } = cursorHiding;

  return {
    // Zoom
    editingKeyframeId,
    setEditingKeyframeId,
    zoomFactor,
    setZoomFactor,
    handleAddKeyframe,
    handleDeleteKeyframe,
    throttledUpdateZoom,
    // Text
    editingTextId,
    setEditingTextId,
    handleAddText,
    handleDeleteText,
    handleTextDragMove,
    // Cursor/pointer
    editingPointerId,
    setEditingPointerId,
    handleSmartPointerHiding,
    handleAddPointerSegment,
    handleDeletePointerSegment,
    // Auto zoom (no extra exports needed, just the single handler)
    handleAutoZoom,
  };
}
