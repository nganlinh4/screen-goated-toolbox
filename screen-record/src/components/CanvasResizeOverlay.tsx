import { useCallback, useEffect, useRef, useState } from 'react';
import { BackgroundConfig } from '@/types/video';

interface CanvasResizeOverlayProps {
  previewContainerRef: React.RefObject<HTMLDivElement>;
  backgroundConfig: BackgroundConfig;
  setBackgroundConfig: React.Dispatch<React.SetStateAction<BackgroundConfig>>;
  beginBatch: () => void;
  commitBatch: () => void;
  onDragStateChange?: (dragging: boolean) => void;
}

export function CanvasResizeOverlay({
  previewContainerRef,
  backgroundConfig,
  setBackgroundConfig,
  beginBatch,
  commitBatch,
  onDragStateChange
}: CanvasResizeOverlayProps) {
  const moveListenerRef = useRef<((event: MouseEvent) => void) | null>(null);
  const upListenerRef = useRef<((event: MouseEvent) => void) | null>(null);
  const dragActiveRef = useRef(false);
  const labelHideTimeoutRef = useRef<number | null>(null);
  const [isLabelVisible, setIsLabelVisible] = useState(false);

  const clearWindowDragListeners = useCallback((
    shouldCommitBatch: boolean,
    pendingAnimationFrameId?: number | null,
  ) => {
    if (pendingAnimationFrameId !== null && pendingAnimationFrameId !== undefined) {
      cancelAnimationFrame(pendingAnimationFrameId);
    }
    if (moveListenerRef.current) {
      window.removeEventListener('mousemove', moveListenerRef.current);
      moveListenerRef.current = null;
    }
    if (upListenerRef.current) {
      window.removeEventListener('mouseup', upListenerRef.current);
      upListenerRef.current = null;
    }
    if (dragActiveRef.current) {
      dragActiveRef.current = false;
      onDragStateChange?.(false);
      if (shouldCommitBatch) {
        commitBatch();
      }
      if (labelHideTimeoutRef.current !== null) {
        window.clearTimeout(labelHideTimeoutRef.current);
      }
      labelHideTimeoutRef.current = window.setTimeout(() => {
        setIsLabelVisible(false);
        labelHideTimeoutRef.current = null;
      }, 1000);
    }
  }, [commitBatch, onDragStateChange]);

  useEffect(() => {
    return () => {
      if (labelHideTimeoutRef.current !== null) {
        window.clearTimeout(labelHideTimeoutRef.current);
        labelHideTimeoutRef.current = null;
      }
      clearWindowDragListeners(true);
    };
  }, [clearWindowDragListeners]);

  useEffect(() => {
    const handleAbortInteractions = () => {
      clearWindowDragListeners(false);
      if (labelHideTimeoutRef.current !== null) {
        window.clearTimeout(labelHideTimeoutRef.current);
        labelHideTimeoutRef.current = null;
      }
      setIsLabelVisible(false);
    };

    window.addEventListener("sr-abort-editor-interactions", handleAbortInteractions);
    return () => {
      window.removeEventListener(
        "sr-abort-editor-interactions",
        handleAbortInteractions,
      );
    };
  }, [clearWindowDragListeners]);

  const container = previewContainerRef.current;
  if (!container) return null;

  const containerRect = container.getBoundingClientRect();
  const containerW = containerRect.width;
  const containerH = containerRect.height;
  const canvasW = backgroundConfig.canvasWidth || 1920;
  const canvasH = backgroundConfig.canvasHeight || 1080;

  if (canvasW <= 0 || canvasH <= 0 || containerW <= 0 || containerH <= 0) return null;

  // Compute displayed canvas rect (CSS max-w-full max-h-full + centered)
  const scale = Math.min(containerW / canvasW, containerH / canvasH, 1);
  const dispW = canvasW * scale;
  const dispH = canvasH * scale;
  const oLeft = (containerW - dispW) / 2;
  const oTop = (containerH - dispH) / 2;

  const handleDragStart = (e: React.MouseEvent, type: string) => {
    e.preventDefault();
    e.stopPropagation();
    clearWindowDragListeners(false);
    if (labelHideTimeoutRef.current !== null) {
      window.clearTimeout(labelHideTimeoutRef.current);
      labelHideTimeoutRef.current = null;
    }
    setIsLabelVisible(true);
    dragActiveRef.current = true;
    onDragStateChange?.(true);
    beginBatch();

    const startX = e.clientX;
    const startY = e.clientY;
    const startW = canvasW;
    const startH = canvasH;
    // Canvas pixels per screen pixel -- x2 because canvas is centered (both sides grow equally)
    const pxPerCanvas = scale > 0 ? 1 / scale : 1;

    let rafId: number | null = null;
    let latestEvent: MouseEvent | null = null;

    const handleMove = (me: MouseEvent) => {
      latestEvent = me;
      if (rafId !== null) return;
      rafId = requestAnimationFrame(() => {
        rafId = null;
        const evt = latestEvent;
        if (!evt) return;
        const dx = evt.clientX - startX;
        const dy = evt.clientY - startY;
        let newW = startW;
        let newH = startH;

        if (type.includes('e')) newW = startW + dx * pxPerCanvas * 2;
        if (type.includes('w')) newW = startW - dx * pxPerCanvas * 2;
        if (type.includes('s')) newH = startH + dy * pxPerCanvas * 2;
        if (type.includes('n')) newH = startH - dy * pxPerCanvas * 2;

        // Clamp to reasonable bounds, ensure even (for ffmpeg yuv420p)
        newW = Math.max(100, Math.min(7680, Math.round(newW)));
        newH = Math.max(100, Math.min(4320, Math.round(newH)));
        if (newW % 2 !== 0) newW++;
        if (newH % 2 !== 0) newH++;

        setBackgroundConfig(prev => ({ ...prev, canvasWidth: newW, canvasHeight: newH }));
      });
    };

    const handleUp = () => {
      clearWindowDragListeners(true, rafId);
    };
    moveListenerRef.current = handleMove;
    upListenerRef.current = handleUp;
    window.addEventListener('mousemove', handleMove);
    window.addEventListener('mouseup', handleUp);
  };

  const handles = [
    { t: 'n',  cursor: 'cursor-n-resize',  x: dispW / 2, y: 0 },
    { t: 's',  cursor: 'cursor-s-resize',  x: dispW / 2, y: dispH },
    { t: 'w',  cursor: 'cursor-w-resize',  x: 0,         y: dispH / 2 },
    { t: 'e',  cursor: 'cursor-e-resize',  x: dispW,     y: dispH / 2 },
    { t: 'nw', cursor: 'cursor-nw-resize', x: 0,         y: 0 },
    { t: 'ne', cursor: 'cursor-ne-resize', x: dispW,     y: 0 },
    { t: 'sw', cursor: 'cursor-sw-resize', x: 0,         y: dispH },
    { t: 'se', cursor: 'cursor-se-resize', x: dispW,     y: dispH },
  ];
  const showLabelBelowTopHandle = oTop < 28;

  return (
    <div className="canvas-resize-overlay absolute inset-0 z-10 pointer-events-none">
      <div
        className="canvas-resize-bounds"
        style={{ position: 'absolute', left: oLeft, top: oTop, width: dispW, height: dispH }}
      >
        <div className="canvas-resize-border absolute inset-0 border border-dashed border-white/30 pointer-events-none" />
        {isLabelVisible && (
          <div
            className={`canvas-resize-label absolute left-1/2 -translate-x-1/2 bg-black/60 text-white/80 text-[9px] px-1.5 py-0.5 rounded whitespace-nowrap pointer-events-none tabular-nums ${
              showLabelBelowTopHandle ? 'top-3' : '-top-5'
            }`}
          >
            {canvasW} x {canvasH}
          </div>
        )}
        {handles.map(handle => (
          <div
            key={handle.t}
            className={`canvas-resize-handle absolute w-2.5 h-2.5 bg-white/80 border border-white/50 rounded-full -translate-x-1/2 -translate-y-1/2 ${handle.cursor} pointer-events-auto hover:scale-150 transition-transform`}
            style={{ left: handle.x, top: handle.y }}
            onMouseDown={(e) => handleDragStart(e, handle.t)}
          />
        ))}
      </div>
    </div>
  );
}
