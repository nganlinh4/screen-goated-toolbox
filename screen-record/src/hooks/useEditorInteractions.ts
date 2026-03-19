import { useRef, type MutableRefObject, type RefObject } from "react";
import { useAppShortcuts } from "@/hooks/useAppShortcuts";
import { useSegmentInitializer } from "@/hooks/useSegmentInitializer";
import { useKeystrokeDrag } from "@/hooks/useKeystrokeDrag";
import type { BackgroundConfig, MousePosition, VideoSegment } from "@/types/video";
import type { ActivePanel } from "@/components/sidepanel/index";

export interface UseEditorInteractionsParams {
  // Shared
  segment: VideoSegment | null;
  setSegment: (s: VideoSegment | null) => void;
  currentTime: number;
  duration: number;
  backgroundConfig: BackgroundConfig;
  canvasRef: RefObject<HTMLCanvasElement | null>;
  videoRef: RefObject<HTMLVideoElement | null>;
  // useAppShortcuts
  seek: (time: number) => void;
  isCropping: boolean;
  isModalOpen: boolean;
  editingKeyframeId: number | null;
  editingTextId: string | null;
  editingKeystrokeSegmentId: string | null;
  editingPointerId: string | null;
  setEditingKeyframeId: (id: number | null) => void;
  handleDeleteText: () => void;
  handleDeleteKeystrokeSegment: () => void;
  handleDeletePointerSegment: () => void;
  canUndo: boolean;
  canRedo: boolean;
  undo: () => void;
  redo: () => void;
  setSeekIndicatorKey: (key: number) => void;
  setSeekIndicatorDir: (dir: "left" | "right") => void;
  handleTogglePlayPause: () => void;
  // useSegmentInitializer
  mousePositions: MousePosition[];
  currentMicAudio: string | null;
  currentWebcamVideo: string | null;
  tempCanvasRef: RefObject<HTMLCanvasElement | null>;
  // useKeystrokeDrag
  segmentRef: MutableRefObject<VideoSegment | null>;
  isDraggingKeystrokeOverlayRef: MutableRefObject<boolean>;
  isResizingKeystrokeOverlayRef: MutableRefObject<boolean>;
  getKeystrokeTimelineDuration: (s: VideoSegment) => number;
  setIsPreviewDragging: (dragging: boolean) => void;
  setIsKeystrokeResizeDragging: (dragging: boolean) => void;
  setIsKeystrokeResizeHandleHover: (hover: boolean) => void;
  setIsKeystrokeOverlaySelected: (selected: boolean) => void;
  setEditingTextId: (id: string | null) => void;
  setActivePanel: (panel: ActivePanel) => void;
  handleTextDragMove: (id: string, x: number, y: number) => void;
  beginBatch: () => void;
  commitBatch: () => void;
}

export function useEditorInteractions({
  segment,
  setSegment,
  currentTime,
  duration,
  backgroundConfig,
  canvasRef,
  videoRef,
  seek,
  isCropping,
  isModalOpen,
  editingKeyframeId,
  editingTextId,
  editingKeystrokeSegmentId,
  editingPointerId,
  setEditingKeyframeId,
  handleDeleteText,
  handleDeleteKeystrokeSegment,
  handleDeletePointerSegment,
  canUndo,
  canRedo,
  undo,
  redo,
  setSeekIndicatorKey,
  setSeekIndicatorDir,
  handleTogglePlayPause,
  mousePositions,
  currentMicAudio,
  currentWebcamVideo,
  tempCanvasRef,
  segmentRef,
  isDraggingKeystrokeOverlayRef,
  isResizingKeystrokeOverlayRef,
  getKeystrokeTimelineDuration,
  setIsPreviewDragging,
  setIsKeystrokeResizeDragging,
  setIsKeystrokeResizeHandleHover,
  setIsKeystrokeOverlaySelected,
  setEditingTextId,
  setActivePanel,
  handleTextDragMove,
  beginBatch,
  commitBatch,
}: UseEditorInteractionsParams) {
  const keystrokeOverlayDragStartRef = useRef<{
    pointerX: number;
    pointerY: number;
    anchorXPx: number;
    baselineYPx: number;
    startScale: number;
    centerX: number;
    centerY: number;
    startRadius: number;
  } | null>(null);

  useAppShortcuts({
    togglePlayPause: handleTogglePlayPause,
    currentTime,
    duration,
    seek,
    isCropping,
    isModalOpen,
    editingKeyframeId,
    editingTextId,
    editingKeystrokeSegmentId,
    editingPointerId,
    segment,
    setSegment,
    setEditingKeyframeId,
    handleDeleteText,
    handleDeleteKeystrokeSegment,
    handleDeletePointerSegment,
    canUndo,
    canRedo,
    undo,
    redo,
    setSeekIndicatorKey,
    setSeekIndicatorDir,
  });

  useSegmentInitializer({
    duration,
    segment,
    backgroundConfig,
    mousePositions,
    currentMicAudio,
    currentWebcamVideo,
    setSegment,
    videoRef,
    canvasRef,
    tempCanvasRef,
  });

  useKeystrokeDrag({
    segment,
    setSegment,
    canvasRef,
    segmentRef,
    isDraggingKeystrokeOverlayRef,
    isResizingKeystrokeOverlayRef,
    keystrokeOverlayDragStartRef,
    currentTime,
    getKeystrokeTimelineDuration,
    setIsPreviewDragging,
    setIsKeystrokeResizeDragging,
    setIsKeystrokeResizeHandleHover,
    setIsKeystrokeOverlaySelected,
    setEditingTextId,
    setActivePanel,
    handleTextDragMove,
    beginBatch,
    commitBatch,
  });
}
