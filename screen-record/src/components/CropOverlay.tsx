import { useLayoutEffect, useState } from 'react';
import { VideoSegment, BackgroundConfig, MousePosition } from '@/types/video';
import { getContainedRect, sampleCaptureDimensionsAtTime } from '@/lib/dynamicCapture';

interface CropOverlayProps {
  segment: VideoSegment;
  mousePositions: MousePosition[];
  currentTime: number;
  previewContainerRef: React.RefObject<HTMLDivElement>;
  canvasRef: React.RefObject<HTMLCanvasElement>;
  videoRef: React.RefObject<HTMLVideoElement>;
  backgroundConfig: BackgroundConfig;
  onUpdateSegment: (segment: VideoSegment) => void;
  beginBatch: () => void;
  commitBatch: () => void;
}

export function CropOverlay({
  segment,
  mousePositions,
  currentTime,
  previewContainerRef,
  canvasRef,
  videoRef,
  backgroundConfig,
  onUpdateSegment,
  beginBatch,
  commitBatch
}: CropOverlayProps) {
  const [canvasBounds, setCanvasBounds] = useState<{
    left: number;
    top: number;
    width: number;
    height: number;
  } | null>(null);

  useLayoutEffect(() => {
    const container = previewContainerRef.current;
    const canvas = canvasRef.current;
    if (!container || !canvas) {
      setCanvasBounds(null);
      return;
    }

    let rafId: number | null = null;

    const updateBounds = () => {
      if (rafId !== null) cancelAnimationFrame(rafId);
      rafId = requestAnimationFrame(() => {
        const nextContainer = previewContainerRef.current;
        const nextCanvas = canvasRef.current;
        const nextVideo = videoRef.current;
        if (!nextContainer || !nextCanvas) {
          setCanvasBounds(null);
          return;
        }
        const containerRect = nextContainer.getBoundingClientRect();
        const canvasRect = nextCanvas.getBoundingClientRect();
        if (canvasRect.width <= 0 || canvasRect.height <= 0) {
          setCanvasBounds(null);
          return;
        }
        const videoWidth = nextVideo?.videoWidth || nextCanvas.width;
        const videoHeight = nextVideo?.videoHeight || nextCanvas.height;
        const canvasLeft = canvasRect.left - containerRect.left;
        const canvasTop = canvasRect.top - containerRect.top;
        const contentScale = Math.max(0.01, (backgroundConfig.scale ?? 100) / 100);
        let nextBounds = {
          left: canvasLeft,
          top: canvasTop,
          width: canvasRect.width,
          height: canvasRect.height,
        };

        if (videoWidth > 0 && videoHeight > 0) {
          const captureDims = sampleCaptureDimensionsAtTime(
            currentTime,
            mousePositions,
            videoWidth,
            videoHeight,
          );
          const contained = getContainedRect(
            canvasRect.width,
            canvasRect.height,
            captureDims.width,
            captureDims.height,
            contentScale,
          );
          nextBounds = {
            left: canvasLeft + contained.left,
            top: canvasTop + contained.top,
            width: contained.width,
            height: contained.height,
          };
        }

        setCanvasBounds((prev) => {
          if (
            prev &&
            Math.abs(prev.left - nextBounds.left) < 0.5 &&
            Math.abs(prev.top - nextBounds.top) < 0.5 &&
            Math.abs(prev.width - nextBounds.width) < 0.5 &&
            Math.abs(prev.height - nextBounds.height) < 0.5
          ) {
            return prev;
          }
          return nextBounds;
        });
      });
    };

    updateBounds();

    const resizeObserver = new ResizeObserver(() => updateBounds());
    resizeObserver.observe(container);
    resizeObserver.observe(canvas);
    window.addEventListener('resize', updateBounds);
    window.addEventListener('scroll', updateBounds, true);

    return () => {
      if (rafId !== null) cancelAnimationFrame(rafId);
      resizeObserver.disconnect();
      window.removeEventListener('resize', updateBounds);
      window.removeEventListener('scroll', updateBounds, true);
    };
  }, [
    previewContainerRef,
    canvasRef,
    videoRef,
    currentTime,
    mousePositions,
    backgroundConfig.scale,
  ]);

  if (!canvasBounds) return null;
  const crop = segment.crop || { x: 0, y: 0, width: 1, height: 1 };
  const renderW = canvasBounds.width;
  const renderH = canvasBounds.height;
  const renderLeft = canvasBounds.left;
  const renderTop = canvasBounds.top;

  const handleResizeStart = (e: React.MouseEvent, type: string) => {
    e.preventDefault();
    e.stopPropagation();
    beginBatch();
    const startX = e.clientX;
    const startY = e.clientY;
    const startCrop = { ...crop };

    const handleMove = (me: MouseEvent) => {
      const dx = me.clientX - startX;
      const dy = me.clientY - startY;
      const dXPct = dx / renderW;
      const dYPct = dy / renderH;

      let newX = startCrop.x;
      let newY = startCrop.y;
      let newW = startCrop.width;
      let newH = startCrop.height;

      if (type.includes('n')) {
        let desiredY = startCrop.y + dYPct;
        const maxY = startCrop.y + startCrop.height - 0.05;
        desiredY = Math.max(0, Math.min(maxY, desiredY));
        const deltaY = desiredY - startCrop.y;
        newY = desiredY;
        newH = startCrop.height - deltaY;
      } else if (type.includes('s')) {
        const desiredH = startCrop.height + dYPct;
        newH = Math.max(0.05, Math.min(1 - startCrop.y, desiredH));
      }

      if (type.includes('w')) {
        let desiredX = startCrop.x + dXPct;
        const maxX = startCrop.x + startCrop.width - 0.05;
        desiredX = Math.max(0, Math.min(maxX, desiredX));
        const deltaX = desiredX - startCrop.x;
        newX = desiredX;
        newW = startCrop.width - deltaX;
      } else if (type.includes('e')) {
        const desiredW = startCrop.width + dXPct;
        newW = Math.max(0.05, Math.min(1 - startCrop.x, desiredW));
      }

      onUpdateSegment({ ...segment, crop: { x: newX, y: newY, width: newW, height: newH } });
    };

    const handleUp = () => {
      window.removeEventListener('mousemove', handleMove);
      window.removeEventListener('mouseup', handleUp);
      commitBatch();
    };
    window.addEventListener('mousemove', handleMove);
    window.addEventListener('mouseup', handleUp);
  };

  const handleBoxMove = (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    beginBatch();
    const startX = e.clientX;
    const startY = e.clientY;
    const startCrop = { ...crop };

    const handleMove = (me: MouseEvent) => {
      const dx = (me.clientX - startX) / renderW;
      const dy = (me.clientY - startY) / renderH;
      const newX = Math.max(0, Math.min(1 - startCrop.width, startCrop.x + dx));
      const newY = Math.max(0, Math.min(1 - startCrop.height, startCrop.y + dy));
      onUpdateSegment({ ...segment, crop: { x: newX, y: newY, width: startCrop.width, height: startCrop.height } });
    };

    const handleUp = () => {
      window.removeEventListener('mousemove', handleMove);
      window.removeEventListener('mouseup', handleUp);
      commitBatch();
    };
    window.addEventListener('mousemove', handleMove);
    window.addEventListener('mouseup', handleUp);
  };

  const handles = [
    { t: 'nw', c: 'cursor-nw-resize', s: '-top-1.5 -left-1.5' },
    { t: 'n', c: 'cursor-n-resize', s: '-top-1.5 left-1/2 -translate-x-1/2' },
    { t: 'ne', c: 'cursor-ne-resize', s: '-top-1.5 -right-1.5' },
    { t: 'w', c: 'cursor-w-resize', s: 'top-1/2 -translate-y-1/2 -left-1.5' },
    { t: 'e', c: 'cursor-e-resize', s: 'top-1/2 -translate-y-1/2 -right-1.5' },
    { t: 'sw', c: 'cursor-sw-resize', s: '-bottom-1.5 -left-1.5' },
    { t: 's', c: 'cursor-s-resize', s: '-bottom-1.5 left-1/2 -translate-x-1/2' },
    { t: 'se', c: 'cursor-se-resize', s: '-bottom-1.5 -right-1.5' },
  ];

  return (
    <div className="crop-overlay-container absolute inset-0 z-20 pointer-events-none">
      <div className="crop-video-bounds" style={{ position: 'absolute', left: renderLeft, top: renderTop, width: renderW, height: renderH }}>
        <div
          className="crop-selection-box absolute border-2 border-[var(--primary-color)] bg-[var(--primary-color)]/10 pointer-events-auto"
          style={{
            left: `${crop.x * 100}%`,
            top: `${crop.y * 100}%`,
            width: `${crop.width * 100}%`,
            height: `${crop.height * 100}%`,
            boxShadow: '0 0 0 9999px rgba(0, 0, 0, 0.7)'
          }}
          onMouseDown={handleBoxMove}
        >
          {/* Grid Lines */}
          <div className="crop-grid-rows absolute inset-0 flex flex-col pointer-events-none opacity-30">
            <div className="flex-1 border-b border-white/50" />
            <div className="flex-1 border-b border-white/50" />
            <div className="flex-1" />
          </div>
          <div className="crop-grid-cols absolute inset-0 flex pointer-events-none opacity-30">
            <div className="flex-1 border-r border-white/50" />
            <div className="flex-1 border-r border-white/50" />
            <div className="flex-1" />
          </div>

          {/* Handles */}
          {handles.map(handle => (
            <div
              key={handle.t}
              className={`crop-handle absolute w-3 h-3 bg-white border border-[var(--primary-color)] rounded-full z-30 hover:scale-125 transition-transform ${handle.c} ${handle.s}`}
              onMouseDown={(e) => handleResizeStart(e, handle.t)}
            />
          ))}

          {/* Central Crosshair */}
          <div className="crop-crosshair absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 w-4 h-4 opacity-50 pointer-events-none">
            <div className="crosshair-h absolute w-full h-[1px] bg-white top-1/2 -translate-y-1/2 shadow-sm" />
            <div className="crosshair-v absolute h-full w-[1px] bg-white left-1/2 -translate-x-1/2 shadow-sm" />
          </div>
        </div>
      </div>
    </div>
  );
}
