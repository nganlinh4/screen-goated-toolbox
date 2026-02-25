import { useEffect, type RefObject, type MutableRefObject } from "react";
import { videoRenderer } from '@/lib/videoRenderer';
import { VideoSegment } from '@/types/video';
import type { ActivePanel } from '@/components/sidepanel/index';

interface KeystrokeOverlayDragStart {
  pointerX: number;
  pointerY: number;
  anchorXPx: number;
  baselineYPx: number;
  startScale: number;
  centerX: number;
  centerY: number;
  startRadius: number;
}

export interface UseKeystrokeDragParams {
  segment: VideoSegment | null;
  setSegment: (segment: VideoSegment) => void;
  canvasRef: RefObject<HTMLCanvasElement | null>;
  segmentRef: MutableRefObject<VideoSegment | null>;
  isDraggingKeystrokeOverlayRef: MutableRefObject<boolean>;
  isResizingKeystrokeOverlayRef: MutableRefObject<boolean>;
  keystrokeOverlayDragStartRef: MutableRefObject<KeystrokeOverlayDragStart | null>;
  currentTime: number;
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

export function useKeystrokeDrag({
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
}: UseKeystrokeDragParams) {
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || !segment) return;
    const onDown = (e: MouseEvent) => {
      const liveSegment = segmentRef.current;
      if (!liveSegment) return;
      const rect = canvas.getBoundingClientRect();
      const x = (e.clientX - rect.left) * (canvas.width / rect.width);
      const y = (e.clientY - rect.top) * (canvas.height / rect.height);
      const keystrokeMode = liveSegment.keystrokeMode ?? 'off';
      if (keystrokeMode !== 'off') {
        const editBounds = videoRenderer.getKeystrokeOverlayEditBounds(
          liveSegment,
          canvas,
          currentTime,
          getKeystrokeTimelineDuration(liveSegment)
        );
        if (editBounds) {
          const handleSize = editBounds.handleSize;
          const handleX = editBounds.x + editBounds.width - handleSize;
          const handleY = editBounds.y + editBounds.height - handleSize;
          const inBox = x >= editBounds.x && x <= editBounds.x + editBounds.width
            && y >= editBounds.y && y <= editBounds.y + editBounds.height;
          const inHandle = x >= handleX && x <= handleX + handleSize
            && y >= handleY && y <= handleY + handleSize;
          if (inBox || inHandle) {
            const overlay = videoRenderer.getKeystrokeOverlayConfig(liveSegment);
            const anchorXPx = (overlay.x / 100) * canvas.width;
            const baselineYPx = (overlay.y / 100) * canvas.height;
            const centerX = editBounds.x + editBounds.width / 2;
            const centerY = editBounds.y + editBounds.height / 2;
            const radius = Math.max(6, Math.hypot(x - centerX, y - centerY));
            keystrokeOverlayDragStartRef.current = {
              pointerX: x,
              pointerY: y,
              anchorXPx,
              baselineYPx,
              startScale: overlay.scale,
              centerX,
              centerY,
              startRadius: radius,
            };
            isDraggingKeystrokeOverlayRef.current = !inHandle;
            isResizingKeystrokeOverlayRef.current = inHandle;
            setIsPreviewDragging(true);
            setIsKeystrokeResizeDragging(inHandle);
            setIsKeystrokeResizeHandleHover(inHandle);
            setIsKeystrokeOverlaySelected(true);
            beginBatch();
            e.stopPropagation();
            e.preventDefault();
            return;
          }
        }
      }
      const hitId = videoRenderer.handleMouseDown(e, segment, canvas);
      if (hitId) {
        e.stopPropagation();
        e.preventDefault();
        setEditingTextId(hitId);
        setActivePanel('text');
        setIsPreviewDragging(true);
        setIsKeystrokeOverlaySelected(false);
      } else {
        setIsKeystrokeResizeHandleHover(false);
        setIsKeystrokeOverlaySelected(false);
      }
    };
    const onMove = (e: MouseEvent) => {
      const liveSegment = segmentRef.current;
      if (!liveSegment) return;
      const mode = liveSegment.keystrokeMode ?? 'off';
      if (
        !isDraggingKeystrokeOverlayRef.current &&
        !isResizingKeystrokeOverlayRef.current &&
        mode !== 'off'
      ) {
        const rect = canvas.getBoundingClientRect();
        const x = (e.clientX - rect.left) * (canvas.width / rect.width);
        const y = (e.clientY - rect.top) * (canvas.height / rect.height);
        const editBounds = videoRenderer.getKeystrokeOverlayEditBounds(
          liveSegment,
          canvas,
          currentTime,
          getKeystrokeTimelineDuration(liveSegment)
        );
        if (editBounds) {
          const handleSize = editBounds.handleSize;
          const handleX = editBounds.x + editBounds.width - handleSize;
          const handleY = editBounds.y + editBounds.height - handleSize;
          const inHandle = x >= handleX && x <= handleX + handleSize
            && y >= handleY && y <= handleY + handleSize;
          setIsKeystrokeResizeHandleHover(inHandle);
        } else {
          setIsKeystrokeResizeHandleHover(false);
        }
      }
      const dragState = keystrokeOverlayDragStartRef.current;
      if (dragState && (isDraggingKeystrokeOverlayRef.current || isResizingKeystrokeOverlayRef.current)) {
        const rect = canvas.getBoundingClientRect();
        const x = (e.clientX - rect.left) * (canvas.width / rect.width);
        const y = (e.clientY - rect.top) * (canvas.height / rect.height);
        if (isDraggingKeystrokeOverlayRef.current) {
          const dx = x - dragState.pointerX;
          const dy = y - dragState.pointerY;
          const nextX = Math.max(0, Math.min(100, ((dragState.anchorXPx + dx) / canvas.width) * 100));
          const nextY = Math.max(0, Math.min(100, ((dragState.baselineYPx + dy) / canvas.height) * 100));
          setSegment({
            ...liveSegment,
            keystrokeOverlay: {
              ...videoRenderer.getKeystrokeOverlayConfig(liveSegment),
              x: nextX,
              y: nextY,
            },
          });
        } else if (isResizingKeystrokeOverlayRef.current) {
          const radius = Math.max(6, Math.hypot(x - dragState.centerX, y - dragState.centerY));
          const ratio = radius / Math.max(6, dragState.startRadius);
          const nextScale = Math.max(0.45, Math.min(2.4, dragState.startScale * ratio));
          setSegment({
            ...liveSegment,
            keystrokeOverlay: {
              ...videoRenderer.getKeystrokeOverlayConfig(liveSegment),
              scale: nextScale,
            },
          });
        }
        return;
      }
      videoRenderer.handleMouseMove(e, segment, canvas, handleTextDragMove);
    };
    const onUp = () => {
      const wasOverlayEditing = isDraggingKeystrokeOverlayRef.current || isResizingKeystrokeOverlayRef.current;
      isDraggingKeystrokeOverlayRef.current = false;
      isResizingKeystrokeOverlayRef.current = false;
      keystrokeOverlayDragStartRef.current = null;
      setIsPreviewDragging(false);
      setIsKeystrokeResizeDragging(false);
      setIsKeystrokeResizeHandleHover(false);
      if (wasOverlayEditing) commitBatch();
      videoRenderer.handleMouseUp();
    };
    canvas.addEventListener('mousedown', onDown);
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
    return () => {
      canvas.removeEventListener('mousedown', onDown);
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
    };
  }, [segment, handleTextDragMove, canvasRef, setEditingTextId, setActivePanel, beginBatch, commitBatch, currentTime, getKeystrokeTimelineDuration, setSegment, segmentRef, isDraggingKeystrokeOverlayRef, isResizingKeystrokeOverlayRef, keystrokeOverlayDragStartRef, setIsPreviewDragging, setIsKeystrokeResizeDragging, setIsKeystrokeResizeHandleHover, setIsKeystrokeOverlaySelected]);
}
