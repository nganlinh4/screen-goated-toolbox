import { useEffect, useRef, useState } from 'react';
import { Button } from '@/components/ui/button';
import { AlertTriangle, Loader2, Play, Pause, Crop, Pencil, PhotoCamera, Check, X } from '@/components/ui/MaterialIcon';
import { formatTime } from '@/utils/helpers';
import { useSettings } from '@/hooks/useSettings';
import { notifyUserError } from '@/lib/userNotifications';

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
  onDownloadFrame?: () => Promise<string>;
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
  onDownloadFrame,
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
  const [frameSaveStatus, setFrameSaveStatus] = useState<"idle" | "saving" | "saved" | "error">("idle");
  const frameStatusTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => () => {
    if (frameStatusTimerRef.current) clearTimeout(frameStatusTimerRef.current);
  }, []);

  const handleDownloadFrame = async () => {
    if (!onDownloadFrame || frameSaveStatus === "saving") return;
    if (frameStatusTimerRef.current) clearTimeout(frameStatusTimerRef.current);
    setFrameSaveStatus("saving");
    try {
      await onDownloadFrame();
      setFrameSaveStatus("saved");
    } catch (error) {
      notifyUserError("frameSaveFailed", error);
      setFrameSaveStatus("error");
    }
    frameStatusTimerRef.current = setTimeout(() => setFrameSaveStatus("idle"), 1800);
  };

  const frameButtonLabel = frameSaveStatus === "saving"
    ? t.savingFrame
    : frameSaveStatus === "saved"
      ? t.frameSaved
      : frameSaveStatus === "error"
        ? t.frameSaveFailed
        : t.downloadCurrentFrame;

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
          {onDownloadFrame && (
            <Button
              onClick={() => { void handleDownloadFrame(); }}
              disabled={isProcessing || !isVideoReady || frameSaveStatus === "saving"}
              variant="ghost"
              size="icon"
              className="playback-frame-download-btn ui-action-button w-8 h-8 rounded-lg transition-colors text-[var(--overlay-panel-fg)]/80 hover:text-[var(--overlay-panel-fg)] hover:bg-[var(--ui-hover)]"
              data-tone={frameSaveStatus === "error" ? "danger" : "success"}
              data-active={frameSaveStatus === "saved" || frameSaveStatus === "error" ? "true" : "false"}
              title={frameButtonLabel}
              aria-label={frameButtonLabel}
            >
              {frameSaveStatus === "saving" ? (
                <Loader2 className="w-3.5 h-3.5 animate-spin" />
              ) : frameSaveStatus === "saved" ? (
                <Check className="w-3.5 h-3.5" />
              ) : frameSaveStatus === "error" ? (
                <AlertTriangle className="w-3.5 h-3.5" />
              ) : (
                <PhotoCamera className="w-3.5 h-3.5" />
              )}
            </Button>
          )}
          <div className="control-divider w-px h-5" style={{ backgroundColor: 'var(--overlay-divider)' }} />
        </>
      )}
      <Button
        onClick={onTogglePlayPause}
        disabled={isProcessing || !isVideoReady}
        variant="ghost"
        size="icon"
        aria-label={isPlaying ? t.pause : t.play}
        title={isPlaying ? t.pause : t.play}
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
