import {
  useState,
  useCallback,
  useMemo,
  useRef,
  useEffect,
  type RefObject,
} from "react";
import { videoRenderer } from "@/lib/videoRenderer";
import { type ZoomKeyframe } from "@/types/video";
import { type ActivePanel } from "@/components/sidepanel/index";

interface UsePreviewInteractionParams {
  currentVideo: string | null;
  isCropping: boolean;
  activePanel: ActivePanel;
  setActivePanel: (panel: ActivePanel) => void;
  isPlaying: boolean;
  handleTogglePlayPause: () => void;
  handleAddKeyframe: (override?: Partial<ZoomKeyframe>) => void;
  beginBatch: () => void;
  commitBatch: () => void;
  previewContainerRef: RefObject<HTMLDivElement | null>;
  isKeystrokeResizeDragging: boolean;
  isKeystrokeResizeHandleHover: boolean;
}

export function usePreviewInteraction({
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
}: UsePreviewInteractionParams) {
  const [isPreviewDragging, setIsPreviewDragging] = useState(false);
  const [seekIndicatorKey, setSeekIndicatorKey] = useState(0);
  const [seekIndicatorDir, setSeekIndicatorDir] = useState<"left" | "right">(
    "right",
  );

  const previewDragCleanupRef = useRef<(() => void) | null>(null);
  const wheelBatchActiveRef = useRef(false);
  const wheelBatchTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    return () => {
      previewDragCleanupRef.current?.();
    };
  }, []);

  const handlePreviewMouseDown = useCallback(
    (e: React.MouseEvent) => {
      if (!currentVideo || isCropping || activePanel === "text") return;
      e.preventDefault();
      e.stopPropagation();
      if (isPlaying) handleTogglePlayPause();

      const startX = e.clientX;
      const startY = e.clientY;
      const lastState = videoRenderer.getLastCalculatedState();
      if (!lastState) return;

      const {
        positionX: startPosX,
        positionY: startPosY,
        zoomFactor: z,
      } = lastState;
      const rect = e.currentTarget.getBoundingClientRect();
      beginBatch();
      setIsPreviewDragging(true);
      let lockedAxis: "x" | "y" | null = null;

      const handleMouseMove = (me: MouseEvent) => {
        let dx = me.clientX - startX;
        let dy = me.clientY - startY;

        if (me.shiftKey) {
          if (!lockedAxis) {
            if (Math.abs(dx) > Math.abs(dy)) lockedAxis = "x";
            else lockedAxis = "y";
          }
          if (lockedAxis === "x") dy = 0;
          if (lockedAxis === "y") dx = 0;
        } else {
          lockedAxis = null;
        }

        handleAddKeyframe({
          zoomFactor: z,
          positionX: Math.max(0, Math.min(1, startPosX - dx / rect.width / z)),
          positionY: Math.max(0, Math.min(1, startPosY - dy / rect.height / z)),
        });
        setActivePanel("zoom");
      };

      const handleMouseUp = () => {
        window.removeEventListener("mousemove", handleMouseMove);
        window.removeEventListener("mouseup", handleMouseUp);
        previewDragCleanupRef.current = null;
        setIsPreviewDragging(false);
        commitBatch();
      };

      // Store cleanup so unmount can remove listeners if mouseup never fires.
      previewDragCleanupRef.current = () => {
        window.removeEventListener("mousemove", handleMouseMove);
        window.removeEventListener("mouseup", handleMouseUp);
      };

      window.addEventListener("mousemove", handleMouseMove);
      window.addEventListener("mouseup", handleMouseUp);
    },
    [
      currentVideo,
      isCropping,
      activePanel,
      isPlaying,
      handleTogglePlayPause,
      handleAddKeyframe,
      beginBatch,
      commitBatch,
    ],
  );

  const previewCursorClass = useMemo(() => {
    if (isKeystrokeResizeDragging || isKeystrokeResizeHandleHover)
      return "cursor-nwse-resize";
    if (isPreviewDragging) return "cursor-grabbing";
    if (currentVideo && !isCropping) return "cursor-grab";
    return "cursor-default";
  }, [
    isKeystrokeResizeDragging,
    isKeystrokeResizeHandleHover,
    isPreviewDragging,
    currentVideo,
    isCropping,
  ]);

  // Wheel zoom
  useEffect(() => {
    const container = previewContainerRef.current;
    if (!container) return;

    const handleWheel = (e: WheelEvent) => {
      if (!currentVideo || isCropping) return;
      e.preventDefault();
      const lastState = videoRenderer.getLastCalculatedState();
      if (!lastState) return;

      if (!wheelBatchActiveRef.current) {
        beginBatch();
        wheelBatchActiveRef.current = true;
      }
      if (wheelBatchTimerRef.current) clearTimeout(wheelBatchTimerRef.current);
      wheelBatchTimerRef.current = setTimeout(() => {
        commitBatch();
        wheelBatchActiveRef.current = false;
        wheelBatchTimerRef.current = null;
      }, 400);

      const newZoom = Math.max(
        1.0,
        Math.min(
          12.0,
          lastState.zoomFactor - e.deltaY * 0.002 * lastState.zoomFactor,
        ),
      );
      handleAddKeyframe({
        zoomFactor: newZoom,
        positionX: lastState.positionX,
        positionY: lastState.positionY,
      });
      setActivePanel("zoom");
    };

    container.addEventListener("wheel", handleWheel, { passive: false });
    return () => container.removeEventListener("wheel", handleWheel);
  }, [currentVideo, isCropping, handleAddKeyframe, beginBatch, commitBatch, previewContainerRef, setActivePanel]);

  return {
    isPreviewDragging,
    setIsPreviewDragging,
    seekIndicatorKey,
    setSeekIndicatorKey,
    seekIndicatorDir,
    setSeekIndicatorDir,
    previewCursorClass,
    handlePreviewMouseDown,
  };
}
