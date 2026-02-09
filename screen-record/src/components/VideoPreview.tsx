import { forwardRef } from 'react';
import { Button } from '@/components/ui/button';
import { Video, Loader2, Play, Pause, Crop } from 'lucide-react';
import { VideoSegment, BackgroundConfig } from '@/types/video';
import { formatTime } from '@/utils/helpers';
import { useSettings } from '@/hooks/useSettings';

// ============================================================================
// Placeholder
// ============================================================================
interface PlaceholderProps {
  isLoadingVideo: boolean;
  loadingProgress: number;
  isRecording: boolean;
  recordingDuration: number;
}

export function Placeholder({
  isLoadingVideo,
  loadingProgress,
  isRecording,
  recordingDuration
}: PlaceholderProps) {
  const { t } = useSettings();
  return (
    <div className="placeholder-container absolute inset-0 bg-[var(--surface)] flex flex-col items-center justify-center">
      <div className="placeholder-background absolute inset-0 opacity-5">
        <div
          className="grid-pattern w-full h-full"
          style={{
            backgroundImage: `
              linear-gradient(to right, #fff 1px, transparent 1px),
              linear-gradient(to bottom, #fff 1px, transparent 1px)
            `,
            backgroundSize: '20px 20px'
          }}
        />
      </div>

      {isLoadingVideo ? (
        <div className="loading-state flex flex-col items-center">
          <Loader2 className="w-8 h-8 text-[var(--primary-color)] animate-spin mb-3" />
          <p className="text-[var(--on-surface)] text-sm font-medium">{t.processingVideo}</p>
          <p className="text-[var(--outline)] text-xs mt-1">{t.processingHint}</p>
        </div>
      ) : isRecording ? (
        <div className="recording-state flex flex-col items-center">
          <div className="recording-pulse w-3 h-3 rounded-full bg-[var(--tertiary-color)] animate-pulse mb-3" />
          <p className="text-[var(--on-surface)] text-sm font-medium">{t.recordingInProgress}</p>
          <span className="text-[var(--on-surface)] text-lg font-mono mt-2">{formatTime(recordingDuration)}</span>
        </div>
      ) : (
        <div className="no-video-state flex flex-col items-center">
          <Video className="w-8 h-8 text-[var(--outline-variant)] mb-3" />
          <p className="text-[var(--on-surface)] text-sm font-medium">{t.noVideoSelected}</p>
          <p className="text-[var(--outline)] text-xs mt-1">{t.startRecordingHint}</p>
        </div>
      )}
      {isLoadingVideo && loadingProgress > 0 && (
        <div className="loading-progress mt-2">
          <p className="text-[var(--outline)] text-xs">
            {t.loadingVideo} {Math.min(Math.round(loadingProgress), 100)}%
          </p>
        </div>
      )}
    </div>
  );
}

// ============================================================================
// PlaybackControls
// ============================================================================
interface PlaybackControlsProps {
  isPlaying: boolean;
  isProcessing: boolean;
  isVideoReady: boolean;
  isCropping: boolean;
  currentTime: number;
  duration: number;
  onTogglePlayPause: () => void;
  onToggleCrop: () => void;
  autoZoomButton?: React.ReactNode;
  smartPointerButton?: React.ReactNode;
}

export function PlaybackControls({
  isPlaying,
  isProcessing,
  isVideoReady,
  isCropping,
  currentTime,
  duration,
  onTogglePlayPause,
  onToggleCrop,
  autoZoomButton,
  smartPointerButton,
}: PlaybackControlsProps) {
  const { t } = useSettings();
  return (
    <div
      className="playback-controls absolute bottom-4 left-1/2 transform -translate-x-1/2 flex items-center gap-2 backdrop-blur-xl rounded-xl px-3 py-2 border shadow-[0_8px_32px_rgba(0,0,0,0.22)] z-50 whitespace-nowrap"
      style={{
        backgroundColor: 'var(--overlay-panel-bg)',
        borderColor: 'var(--overlay-panel-border)',
        color: 'var(--overlay-panel-fg)',
      }}
    >
      <Button
        onClick={onToggleCrop}
        variant="ghost"
        size="icon"
        className={`w-8 h-8 rounded-lg transition-colors ${
          isCropping
            ? 'bg-green-500/80 text-white hover:bg-green-600'
            : 'text-[var(--overlay-panel-fg)]/80 hover:text-[var(--overlay-panel-fg)] hover:bg-[var(--glass-bg)]'
        }`}
        title={isCropping ? t.applyCrop : t.cropVideo}
      >
        {isCropping ? (
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <polyline points="20 6 9 17 4 12" />
          </svg>
        ) : (
          <Crop className="w-3.5 h-3.5" />
        )}
      </Button>
      {!isCropping && (
        <>
          <div className="control-divider w-px h-5" style={{ backgroundColor: 'var(--overlay-divider)' }} />
          <Button
            onClick={onTogglePlayPause}
            disabled={isProcessing || !isVideoReady}
            variant="ghost"
            size="icon"
            className={`w-8 h-8 rounded-lg transition-colors text-[var(--overlay-panel-fg)] bg-transparent hover:text-[var(--overlay-panel-fg)] hover:bg-[var(--glass-bg)] ${
              isProcessing || !isVideoReady ? 'opacity-50 cursor-not-allowed' : ''
            }`}
          >
            {isPlaying ? <Pause className="w-4 h-4" /> : <Play className="w-4 h-4 ml-0.5" />}
          </Button>
        </>
      )}
      <div className="time-display text-[11px] font-medium tabular-nums flex-shrink-0 text-[var(--overlay-panel-fg)]/90">
        {formatTime(currentTime)} / {formatTime(duration)}
      </div>
      {!isCropping && autoZoomButton && (
        <>
          <div className="control-divider w-px h-5" style={{ backgroundColor: 'var(--overlay-divider)' }} />
          {autoZoomButton}
        </>
      )}
      {!isCropping && smartPointerButton && (
        <>
          <div className="control-divider w-px h-5" style={{ backgroundColor: 'var(--overlay-divider)' }} />
          {smartPointerButton}
        </>
      )}
    </div>
  );
}

// ============================================================================
// CropOverlay
// ============================================================================
interface CropOverlayProps {
  segment: VideoSegment;
  previewContainerRef: React.RefObject<HTMLDivElement>;
  videoRef: React.RefObject<HTMLVideoElement>;
  onUpdateSegment: (segment: VideoSegment) => void;
  beginBatch: () => void;
  commitBatch: () => void;
}

export function CropOverlay({
  segment,
  previewContainerRef,
  videoRef,
  onUpdateSegment,
  beginBatch,
  commitBatch
}: CropOverlayProps) {
  const container = previewContainerRef.current;
  const video = videoRef.current;

  if (!container || !video) return null;

  const containerRect = container.getBoundingClientRect();
  const vidW = video.videoWidth;
  const vidH = video.videoHeight;

  if (!vidW || !vidH) return null;

  const containerRatio = containerRect.width / containerRect.height;
  const videoRatio = vidW / vidH;

  let renderW: number, renderH: number, renderTop: number, renderLeft: number;

  if (containerRatio > videoRatio) {
    renderH = containerRect.height;
    renderW = renderH * videoRatio;
    renderTop = 0;
    renderLeft = (containerRect.width - renderW) / 2;
  } else {
    renderW = containerRect.width;
    renderH = renderW / videoRatio;
    renderLeft = 0;
    renderTop = (containerRect.height - renderH) / 2;
  }

  const crop = segment.crop || { x: 0, y: 0, width: 1, height: 1 };

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
        let desiredH = startCrop.height + dYPct;
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
        let desiredW = startCrop.width + dXPct;
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
      let newX = Math.max(0, Math.min(1 - startCrop.width, startCrop.x + dx));
      let newY = Math.max(0, Math.min(1 - startCrop.height, startCrop.y + dy));
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

// ============================================================================
// CanvasResizeOverlay
// ============================================================================
interface CanvasResizeOverlayProps {
  previewContainerRef: React.RefObject<HTMLDivElement>;
  backgroundConfig: BackgroundConfig;
  setBackgroundConfig: React.Dispatch<React.SetStateAction<BackgroundConfig>>;
  beginBatch: () => void;
  commitBatch: () => void;
}

export function CanvasResizeOverlay({
  previewContainerRef,
  backgroundConfig,
  setBackgroundConfig,
  beginBatch,
  commitBatch
}: CanvasResizeOverlayProps) {
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
    beginBatch();

    const startX = e.clientX;
    const startY = e.clientY;
    const startW = canvasW;
    const startH = canvasH;
    // Canvas pixels per screen pixel — x2 because canvas is centered (both sides grow equally)
    const pxPerCanvas = scale > 0 ? 1 / scale : 1;

    const handleMove = (me: MouseEvent) => {
      const dx = me.clientX - startX;
      const dy = me.clientY - startY;
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
    { t: 'n',  cursor: 'cursor-n-resize',  x: dispW / 2, y: 0 },
    { t: 's',  cursor: 'cursor-s-resize',  x: dispW / 2, y: dispH },
    { t: 'w',  cursor: 'cursor-w-resize',  x: 0,         y: dispH / 2 },
    { t: 'e',  cursor: 'cursor-e-resize',  x: dispW,     y: dispH / 2 },
    { t: 'nw', cursor: 'cursor-nw-resize', x: 0,         y: 0 },
    { t: 'ne', cursor: 'cursor-ne-resize', x: dispW,     y: 0 },
    { t: 'sw', cursor: 'cursor-sw-resize', x: 0,         y: dispH },
    { t: 'se', cursor: 'cursor-se-resize', x: dispW,     y: dispH },
  ];

  return (
    <div className="canvas-resize-overlay absolute inset-0 z-10 pointer-events-none">
      <div
        className="canvas-resize-bounds"
        style={{ position: 'absolute', left: oLeft, top: oTop, width: dispW, height: dispH }}
      >
        <div className="canvas-resize-border absolute inset-0 border border-dashed border-white/30 pointer-events-none" />
        <div className="canvas-resize-label absolute -top-5 left-1/2 -translate-x-1/2 bg-black/60 text-white/80 text-[9px] px-1.5 py-0.5 rounded whitespace-nowrap pointer-events-none tabular-nums">
          {canvasW} x {canvasH}
        </div>
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

// ============================================================================
// VideoPreview (Main Container)
// ============================================================================
interface VideoPreviewProps {
  videoRef: React.RefObject<HTMLVideoElement | null>;
  audioRef: React.RefObject<HTMLAudioElement | null>;
  canvasRef: React.RefObject<HTMLCanvasElement | null>;
  tempCanvasRef: React.RefObject<HTMLCanvasElement>;
  previewContainerRef: React.RefObject<HTMLDivElement>;
  currentVideo: string | null;
  isLoadingVideo: boolean;
  loadingProgress: number;
  isRecording: boolean;
  recordingDuration: number;
  isPlaying: boolean;
  isProcessing: boolean;
  isVideoReady: boolean;
  isCropping: boolean;
  currentTime: number;
  duration: number;
  segment: VideoSegment | null;
  onPreviewMouseDown: (e: React.MouseEvent) => void;
  onTogglePlayPause: () => void;
  onToggleCrop: () => void;
  onUpdateSegment: (segment: VideoSegment) => void;
  beginBatch: () => void;
  commitBatch: () => void;
}

export const VideoPreview = forwardRef<HTMLDivElement, VideoPreviewProps>(({
  videoRef,
  audioRef,
  canvasRef,
  tempCanvasRef,
  previewContainerRef,
  currentVideo,
  isLoadingVideo,
  loadingProgress,
  isRecording,
  recordingDuration,
  isPlaying,
  isProcessing,
  isVideoReady,
  isCropping,
  currentTime,
  duration,
  segment,
  onPreviewMouseDown,
  onTogglePlayPause,
  onToggleCrop,
  onUpdateSegment,
  beginBatch,
  commitBatch
}, _ref) => {
  return (
    <div className="col-span-3 rounded-xl overflow-hidden bg-[var(--surface-container)]/50 flex items-center justify-center">
      <div className="relative w-full flex justify-center max-h-[70vh]">
        <div
          ref={previewContainerRef}
          className={`relative flex items-center justify-center cursor-crosshair group ${!currentVideo ? 'w-full aspect-video' : ''}`}
          onMouseDown={onPreviewMouseDown}
        >
          <canvas ref={canvasRef as React.RefObject<HTMLCanvasElement>} className="max-w-full max-h-[70vh] object-contain" />
          <canvas ref={tempCanvasRef as React.RefObject<HTMLCanvasElement>} className="hidden" />
          <video ref={videoRef as React.RefObject<HTMLVideoElement>} className="hidden" playsInline preload="auto" />
          <audio ref={audioRef as React.RefObject<HTMLAudioElement>} className="hidden" />

          {(!currentVideo || isLoadingVideo) && (
            <Placeholder
              isLoadingVideo={isLoadingVideo}
              loadingProgress={loadingProgress}
              isRecording={isRecording}
              recordingDuration={recordingDuration}
            />
          )}

          {isCropping && currentVideo && segment && (
            <CropOverlay
              segment={segment}
              previewContainerRef={previewContainerRef as React.RefObject<HTMLDivElement>}
              videoRef={videoRef as React.RefObject<HTMLVideoElement>}
              onUpdateSegment={onUpdateSegment}
              beginBatch={beginBatch}
              commitBatch={commitBatch}
            />
          )}
        </div>

        {currentVideo && !isLoadingVideo && (
          <PlaybackControls
            isPlaying={isPlaying}
            isProcessing={isProcessing}
            isVideoReady={isVideoReady}
            isCropping={isCropping}
            currentTime={currentTime}
            duration={duration}
            onTogglePlayPause={onTogglePlayPause}
            onToggleCrop={onToggleCrop}
          />
        )}
      </div>
    </div>
  );
});

VideoPreview.displayName = 'VideoPreview';
