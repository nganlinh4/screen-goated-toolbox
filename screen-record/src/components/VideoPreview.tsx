import { forwardRef, useEffect, useRef, useState } from 'react';
import { Button } from '@/components/ui/button';
import { Video, Loader2, Play, Pause, Crop, Music4, Pencil, Check, X } from '@/components/ui/MaterialIcon';
import { VideoSegment, BackgroundConfig, MousePosition, type ImportedAudioSegment } from '@/types/video';
import { formatTime } from '@/utils/helpers';
import { useSettings } from '@/hooks/useSettings';

// Re-export sub-components for backwards compatibility
export { CropOverlay } from './CropOverlay';
export { CanvasResizeOverlay } from './CanvasResizeOverlay';

// ============================================================================
// Placeholder
// ============================================================================
interface PlaceholderProps {
  isLoadingVideo: boolean;
  loadingProgress: number;
  isRecording: boolean;
  recordingDuration: number;
  onImportVideo?: (file: File) => void;
  /** Imported audio placeholder fallback — show audio info instead of "no video" copy. */
  audioSegments?: ImportedAudioSegment[];
}

export function Placeholder({
  isLoadingVideo,
  loadingProgress,
  isRecording,
  recordingDuration,
  onImportVideo,
  audioSegments,
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
      ) : audioSegments && audioSegments.length > 0 ? (
        <div className="audio-placeholder-state flex flex-col items-center max-w-[80%] text-center">
          <Music4 className="w-10 h-10 text-[var(--primary-color)] mb-3" />
          <p className="text-[var(--on-surface)] text-sm font-medium truncate w-full">
            {audioSegments[0].name}
          </p>
          <p className="text-[var(--outline)] text-xs mt-1 tabular-nums">
            {formatTime(audioSegments[0].duration)}
            {audioSegments.length > 1 ? ` · +${audioSegments.length - 1}` : ''}
          </p>
          <p className="text-[var(--outline)] text-[10px] mt-3 opacity-70">{t.audioPlaceholderProjectHint}</p>
        </div>
      ) : (
        <div className="no-video-state flex flex-col items-center">
          <Video className="w-8 h-8 text-[var(--outline-variant)] mb-3" />
          <p className="text-[var(--on-surface)] text-sm font-medium">{t.noVideoSelected}</p>
          <p className="text-[var(--outline)] text-xs mt-1">{t.startRecordingHint}</p>
          {onImportVideo && (
            <label className="placeholder-import-btn mt-3 text-[10px] text-[var(--outline)] hover:text-[var(--primary-color)] cursor-pointer transition-colors">
              {t.orImportVideo}
              <input type="file" accept="video/*" className="hidden" onChange={(e) => {
                const file = e.target.files?.[0];
                if (file) onImportVideo(file);
                e.target.value = "";
              }} />
            </label>
          )}
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
// SeekIndicator
// ============================================================================
export function SeekIndicator({ dir, showKey }: { dir: 'left' | 'right'; showKey: number }) {
  const [isVisible, setIsVisible] = useState(false);
  const [accumulatedSec, setAccumulatedSec] = useState(5);
  const [animKey, setAnimKey] = useState(0);
  const hideTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const lastDirRef = useRef(dir);

  useEffect(() => {
    if (showKey <= 0) return;

    // Reset accumulation if direction changed or too much time passed
    if (dir !== lastDirRef.current) {
      setAccumulatedSec(5);
    } else if (isVisible) {
      setAccumulatedSec((prev) => prev + 5);
    } else {
      setAccumulatedSec(5);
    }
    lastDirRef.current = dir;

    setIsVisible(true);
    setAnimKey((k) => k + 1);

    // Reset hide timer on each press
    if (hideTimerRef.current) clearTimeout(hideTimerRef.current);
    hideTimerRef.current = setTimeout(() => {
      setIsVisible(false);
      setAccumulatedSec(5);
    }, 700);

    return () => {
      if (hideTimerRef.current) clearTimeout(hideTimerRef.current);
    };
  }, [showKey]); // eslint-disable-line react-hooks/exhaustive-deps

  if (!isVisible) return null;

  const ArrowSvg = ({ flip }: { flip?: boolean }) => (
    <svg
      width="18"
      height="18"
      viewBox="0 0 24 24"
      fill="currentColor"
      style={flip ? { transform: 'scaleX(-1)' } : undefined}
    >
      <path d="M9.575 12L5 7.4L6.4 6l6 6l-6 6L5 16.6zm6.6 0L11.6 7.4L13 6l6 6l-6 6l-1.4-1.4z" />
    </svg>
  );

  return (
    <div className="seek-indicator absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 z-[60] pointer-events-none">
      <div
        key={animKey}
        className="seek-indicator-badge bg-black/75 backdrop-blur-xs text-white px-4 py-2.5 rounded-2xl flex items-center gap-1.5 shadow-2xl"
        style={{
          animation: 'seek-pop 0.35s cubic-bezier(0.34, 1.56, 0.64, 1) forwards',
        }}
      >
        {dir === 'left' && <ArrowSvg flip />}
        <span className="font-bold text-sm tabular-nums">{accumulatedSec}s</span>
        {dir === 'right' && <ArrowSvg />}
      </div>
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
  hasAppliedCrop?: boolean;
  currentTime: number;
  duration: number;
  /** Wall-clock current time (speed-adjusted) for display */
  wallClockCurrentTime?: number;
  /** Wall-clock total duration (speed-adjusted) for display */
  wallClockDuration?: number;
  onTogglePlayPause: () => void;
  onToggleCrop: () => void;
  showCropButton?: boolean;
  onSetProjectDuration?: (duration: number) => void;
  canvasModeToggle?: React.ReactNode;
  keystrokeToggle?: React.ReactNode;
  autoZoomButton?: React.ReactNode;
  smartPointerButton?: React.ReactNode;
  mergeButton?: React.ReactNode;
  selectionChip?: React.ReactNode;
}

export function PlaybackControls({
  isPlaying,
  isProcessing,
  isVideoReady,
  isCropping,
  hasAppliedCrop = false,
  currentTime,
  duration,
  wallClockCurrentTime,
  wallClockDuration,
  onTogglePlayPause,
  onToggleCrop,
  showCropButton = true,
  onSetProjectDuration,
  canvasModeToggle,
  keystrokeToggle,
  autoZoomButton,
  smartPointerButton,
  mergeButton,
  selectionChip,
}: PlaybackControlsProps) {
  const { t } = useSettings();
  const [isEditingDuration, setIsEditingDuration] = useState(false);
  const [durationInput, setDurationInput] = useState("");

  const openDurationEditor = () => {
    setDurationInput((wallClockDuration ?? duration).toFixed(2));
    setIsEditingDuration(true);
  };
  const applyDurationEdit = () => {
    const nextDuration = Number.parseFloat(durationInput.replace(",", "."));
    if (Number.isFinite(nextDuration) && nextDuration > 0) {
      onSetProjectDuration?.(nextDuration);
    }
    setIsEditingDuration(false);
  };

  if (isCropping) {
    return (
      <div className="playback-crop-apply-only flex items-center justify-center">
        <Button
          onClick={onToggleCrop}
          variant="ghost"
          size="icon"
          className="playback-crop-apply-btn ui-action-button w-8 h-8 rounded-lg transition-colors"
          data-tone="success"
          data-active="true"
          data-emphasis="strong"
          title={t.applyCrop}
          aria-label={t.applyCrop}
        >
          <Check size={14} />
        </Button>
      </div>
    );
  }

  return (
    <div
      className="playback-controls relative z-20 flex items-center gap-1.5 rounded-2xl px-3.5 py-2.5 border whitespace-nowrap shadow-[var(--shadow-elevation-2)]"
      style={{
        backgroundColor: 'var(--overlay-panel-bg)',
        borderColor: 'var(--overlay-panel-border)',
        color: 'var(--overlay-panel-fg)',
        boxShadow: 'var(--shadow-elevation-2)',
      }}
    >
      {canvasModeToggle && (
        <>
          {canvasModeToggle}
          <div className="control-divider w-px h-5" style={{ backgroundColor: 'var(--overlay-divider)' }} />
        </>
      )}
      {showCropButton && (
        <>
          <Button
            onClick={onToggleCrop}
            variant="ghost"
            size="icon"
            className={`playback-crop-toggle-btn ui-action-button w-8 h-8 rounded-lg transition-colors ${
              hasAppliedCrop
                ? ''
                : 'text-[var(--overlay-panel-fg)]/80 hover:text-[var(--overlay-panel-fg)] hover:bg-[var(--ui-hover)]'
            }`}
            data-tone="success"
            data-active={hasAppliedCrop ? "true" : "false"}
            title={t.cropVideo}
          >
            <Crop className="w-3.5 h-3.5" />
          </Button>
          <div className="control-divider w-px h-5" style={{ backgroundColor: 'var(--overlay-divider)' }} />
        </>
      )}
      <Button
        onClick={onTogglePlayPause}
        disabled={isProcessing || !isVideoReady}
        variant="ghost"
        size="icon"
        className={`w-8 h-8 rounded-lg transition-colors text-[var(--overlay-panel-fg)] bg-transparent hover:text-[var(--overlay-panel-fg)] hover:bg-[var(--ui-hover)] ${
          isProcessing || !isVideoReady ? 'opacity-50 cursor-not-allowed' : ''
        }`}
      >
        {isPlaying ? <Pause className="w-4 h-4" /> : <Play className="w-4 h-4 ml-0.5" />}
      </Button>
      <div className="time-display group/time-display relative text-[11px] font-medium tabular-nums shrink-0 text-[var(--overlay-panel-fg)]/90">
        {formatTime(wallClockCurrentTime ?? currentTime)} / {formatTime(wallClockDuration ?? duration)}
        {onSetProjectDuration && (
          <>
            <button
              type="button"
              onClick={openDurationEditor}
              className="time-display-duration-edit ui-icon-button absolute -right-7 top-1/2 z-30 h-5 w-5 -translate-y-1/2 rounded-full bg-[var(--surface)]/95 opacity-0 shadow-xs transition-opacity duration-150 group-hover/time-display:opacity-100 focus-visible:opacity-100"
              title={t.editProjectDuration}
              aria-label={t.editProjectDuration}
            >
              <Pencil className="h-3 w-3" />
            </button>
            {isEditingDuration && (
              <div className="time-display-duration-popover absolute left-1/2 top-[calc(100%+8px)] z-40 flex -translate-x-1/2 items-center gap-1 rounded-xl border bg-[var(--surface)] p-2 shadow-[var(--shadow-elevation-2)]">
                <input
                  className="time-display-duration-input h-7 w-20 rounded-lg border border-[var(--ui-border)] bg-[var(--ui-surface-1)] px-2 text-[11px] text-[var(--on-surface)] outline-hidden"
                  value={durationInput}
                  autoFocus
                  inputMode="decimal"
                  onChange={(event) => setDurationInput(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") applyDurationEdit();
                    if (event.key === "Escape") setIsEditingDuration(false);
                  }}
                />
                <Button
                  type="button"
                  onClick={applyDurationEdit}
                  variant="ghost"
                  size="icon"
                  className="time-display-duration-apply ui-action-button h-7 w-7 rounded-lg"
                  title={t.applyProjectDuration}
                >
                  <Check className="h-3.5 w-3.5" />
                </Button>
                <Button
                  type="button"
                  onClick={() => setIsEditingDuration(false)}
                  variant="ghost"
                  size="icon"
                  className="time-display-duration-cancel ui-action-button h-7 w-7 rounded-lg"
                  title={t.cancelProjectDurationEdit}
                >
                  <X className="h-3.5 w-3.5" />
                </Button>
              </div>
            )}
          </>
        )}
      </div>
      {keystrokeToggle && (
        <>
          <div className="control-divider w-px h-5" style={{ backgroundColor: 'var(--overlay-divider)' }} />
          <div className="playback-keystroke-toggle-slot relative group/playback-keystroke flex items-center">
            {keystrokeToggle}
          </div>
        </>
      )}
      {autoZoomButton && (
        <>
          <div className="control-divider w-px h-5" style={{ backgroundColor: 'var(--overlay-divider)' }} />
          <div className="playback-auto-zoom-slot relative group/playback-auto-zoom flex items-center">
            {autoZoomButton}
          </div>
        </>
      )}
      {smartPointerButton && (
        <>
          <div className="control-divider w-px h-5" style={{ backgroundColor: 'var(--overlay-divider)' }} />
          {smartPointerButton}
        </>
      )}
      {mergeButton && (
        <>
          <div className="control-divider w-px h-5" style={{ backgroundColor: 'var(--overlay-divider)' }} />
          {mergeButton}
        </>
      )}
      {selectionChip && (
        <>
          <div className="control-divider w-px h-5" style={{ backgroundColor: 'var(--overlay-divider)' }} />
          {selectionChip}
        </>
      )}
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
  tempCanvasRef: React.RefObject<HTMLCanvasElement | null>;
  previewContainerRef: React.RefObject<HTMLDivElement | null>;
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
  mousePositions: MousePosition[];
  segment: VideoSegment | null;
  backgroundConfig: BackgroundConfig;
  onPreviewMouseDown: (e: React.MouseEvent) => void;
  onTogglePlayPause: () => void;
  onToggleCrop: () => void;
  onUpdateSegment: (segment: VideoSegment) => void;
  beginBatch: () => void;
  commitBatch: () => void;
  seekIndicatorDir?: 'left' | 'right';
  seekIndicatorKey?: number;
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
  mousePositions,
  segment,
  backgroundConfig,
  onPreviewMouseDown,
  onTogglePlayPause,
  onToggleCrop,
  onUpdateSegment,
  beginBatch,
  commitBatch,
  seekIndicatorDir = 'right',
  seekIndicatorKey = 0
}, _ref) => {
  return (
    <div className="video-preview-shell ui-surface col-span-3 rounded-xl overflow-hidden flex items-center justify-center">
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

          {(!currentVideo || isLoadingVideo) && !isRecording && (
            <Placeholder
              isLoadingVideo={isLoadingVideo}
              loadingProgress={loadingProgress}
              isRecording={isRecording}
              recordingDuration={recordingDuration}
            />
          )}

          {isCropping && currentVideo && segment && (
            <CropOverlayInner
              segment={segment}
              mousePositions={mousePositions}
              currentTime={currentTime}
              previewContainerRef={previewContainerRef as React.RefObject<HTMLDivElement>}
              canvasRef={canvasRef as React.RefObject<HTMLCanvasElement>}
              videoRef={videoRef as React.RefObject<HTMLVideoElement>}
              backgroundConfig={backgroundConfig}
              onUpdateSegment={onUpdateSegment}
              beginBatch={beginBatch}
              commitBatch={commitBatch}
            />
          )}
          <SeekIndicator dir={seekIndicatorDir} showKey={seekIndicatorKey} />
        </div>

        {currentVideo && !isLoadingVideo && (
          <PlaybackControls
            isPlaying={isPlaying}
            isProcessing={isProcessing}
            isVideoReady={isVideoReady}
            isCropping={isCropping}
            hasAppliedCrop={Boolean(
              segment?.crop &&
              (
                Math.abs(segment.crop.x) > 0.0005 ||
                Math.abs(segment.crop.y) > 0.0005 ||
                Math.abs(segment.crop.width - 1) > 0.0005 ||
                Math.abs(segment.crop.height - 1) > 0.0005
              )
            )}
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

// Internal import to avoid circular issues -- the CropOverlay component used
// inside VideoPreview is the same one exported from CropOverlay.tsx.
import { CropOverlay as CropOverlayInner } from './CropOverlay';
