import { MousePointer2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { PlaybackControls } from "@/components/VideoPreview";
import { CanvasModeToggle } from "@/components/CanvasModeToggle";
import { KeystrokeToggleControl } from "@/components/KeystrokeToggleControl";
import { AutoZoomControl } from "@/components/AutoZoomControl";
import { useSettings } from "@/hooks/useSettings";
import type { VideoSegment, BackgroundConfig, AutoZoomConfig } from "@/types/video";
import type { CanvasModeToggleProps } from "@/components/CanvasModeToggle";
import type { KeystrokeToggleControlProps } from "@/components/KeystrokeToggleControl";

export interface PlaybackControlsRowProps {
  // Visibility
  showPlaybackControls: boolean;
  showPlaybackControlsGhost: boolean;
  // PlaybackControls props
  isPlaying: boolean;
  isProcessing: boolean;
  isVideoReady: boolean;
  isCropping: boolean;
  hasAppliedCrop: boolean;
  currentTime: number;
  duration: number;
  wallClockCurrentTime: number;
  wallClockDuration: number;
  onTogglePlayPause: () => void;
  onToggleCrop: () => void;
  // CanvasModeToggle props
  backgroundConfig: BackgroundConfig;
  setBackgroundConfig: CanvasModeToggleProps["setBackgroundConfig"];
  customCanvasBaseDimensions: { width: number; height: number };
  getAutoCanvasSelectionConfig: CanvasModeToggleProps["getAutoCanvasSelectionConfig"];
  handleActivateCustomCanvas: () => void;
  handleApplyCanvasRatioPreset: (ratioWidth: number, ratioHeight: number) => void;
  // KeystrokeToggleControl props
  segment: VideoSegment | null;
  setSegment: KeystrokeToggleControlProps["setSegment"];
  handleToggleKeystrokeMode: () => void;
  handleKeystrokeDelayChange: (delay: number) => void;
  // Auto zoom button props
  currentVideo: string | null;
  mousePositionsLength: number;
  handleAutoZoom: () => void;
  autoZoomConfig: AutoZoomConfig;
  handleAutoZoomConfigChange: (config: AutoZoomConfig) => void;
  // Smart pointer button props
  handleSmartPointerHiding: () => void;
  // Selection
  selectedSegmentCount?: number;
  onClearSelection?: () => void;
}

export function PlaybackControlsRow({
  showPlaybackControls,
  showPlaybackControlsGhost,
  isPlaying,
  isProcessing,
  isVideoReady,
  isCropping,
  hasAppliedCrop,
  currentTime,
  duration,
  wallClockCurrentTime,
  wallClockDuration,
  onTogglePlayPause,
  onToggleCrop,
  backgroundConfig,
  setBackgroundConfig,
  customCanvasBaseDimensions,
  getAutoCanvasSelectionConfig,
  handleActivateCustomCanvas,
  handleApplyCanvasRatioPreset,
  segment,
  setSegment,
  handleToggleKeystrokeMode,
  handleKeystrokeDelayChange,
  currentVideo,
  mousePositionsLength,
  handleAutoZoom,
  autoZoomConfig,
  handleAutoZoomConfigChange,
  handleSmartPointerHiding,
  selectedSegmentCount,
  onClearSelection,
}: PlaybackControlsRowProps) {
  const { t } = useSettings();

  const autoZoomDisabled =
    isProcessing ||
    !currentVideo ||
    (!mousePositionsLength && !segment?.smoothMotionPath?.length);

  const smartPointerSegs = segment?.cursorVisibilitySegments;
  const isSmartPointerActive =
    !!smartPointerSegs?.length &&
    !(
      smartPointerSegs.length === 1 &&
      Math.abs(smartPointerSegs[0].startTime - 0) < 0.01 &&
      Math.abs(smartPointerSegs[0].endTime - duration) < 0.01
    );
  const smartPointerDisabled = isProcessing || !currentVideo;
  const smartPointerClass = `smart-pointer-button ui-action-button flex items-center px-2.5 py-1 h-7 text-xs font-medium transition-colors whitespace-nowrap rounded-lg ${
    smartPointerDisabled
      ? "ui-toolbar-button text-[var(--on-surface)]/35 cursor-not-allowed"
      : isSmartPointerActive
        ? ""
        : "ui-chip-button text-[var(--on-surface)]"
  }`;

  return (
    <div
      className={`playback-controls-row flex-shrink-0 flex justify-center pb-1 min-h-[61px] transition-opacity duration-200 ${
        showPlaybackControls || showPlaybackControlsGhost
          ? "opacity-100"
          : "opacity-0 pointer-events-none"
      }`}
    >
      {showPlaybackControls && (
        <PlaybackControls
          isPlaying={isPlaying}
          isProcessing={isProcessing}
          isVideoReady={isVideoReady}
          isCropping={isCropping}
          hasAppliedCrop={hasAppliedCrop}
          currentTime={currentTime}
          duration={duration}
          wallClockCurrentTime={wallClockCurrentTime}
          wallClockDuration={wallClockDuration}
          onTogglePlayPause={onTogglePlayPause}
          onToggleCrop={onToggleCrop}
          canvasModeToggle={
            <CanvasModeToggle
              backgroundConfig={backgroundConfig}
              setBackgroundConfig={setBackgroundConfig}
              customCanvasBaseDimensions={customCanvasBaseDimensions}
              getAutoCanvasSelectionConfig={getAutoCanvasSelectionConfig}
              handleActivateCustomCanvas={handleActivateCustomCanvas}
              handleApplyCanvasRatioPreset={handleApplyCanvasRatioPreset}
            />
          }
          keystrokeToggle={
            <KeystrokeToggleControl
              segment={segment}
              setSegment={setSegment}
              handleToggleKeystrokeMode={handleToggleKeystrokeMode}
              handleKeystrokeDelayChange={handleKeystrokeDelayChange}
            />
          }
          autoZoomButton={
            <AutoZoomControl
              segment={segment}
              disabled={autoZoomDisabled}
              handleAutoZoom={handleAutoZoom}
              autoZoomConfig={autoZoomConfig}
              onConfigChange={handleAutoZoomConfigChange}
            />
          }
          smartPointerButton={
            <Button
              onClick={handleSmartPointerHiding}
              disabled={smartPointerDisabled}
              className={smartPointerClass}
              data-tone="warning"
              data-active={isSmartPointerActive ? "true" : "false"}
            >
              <MousePointer2 className="w-3 h-3 mr-1" />
              {t.smartPointer}
            </Button>
          }
          selectionChip={
            selectedSegmentCount && selectedSegmentCount > 0 && onClearSelection ? (
              <Button
                onClick={onClearSelection}
                className="selection-clear-chip ui-action-button flex items-center px-2 py-1 h-7 text-xs font-medium transition-colors whitespace-nowrap rounded-lg"
                data-tone="accent"
                data-active="true"
              >
                <span className="mr-1">✕</span>
                {selectedSegmentCount} {t.clearSelection}
              </Button>
            ) : null
          }
        />
      )}
      {showPlaybackControlsGhost && (
        <div
          className="editor-empty-playback-chrome relative flex items-center gap-1.5 rounded-2xl border px-3.5 py-2.5 whitespace-nowrap opacity-65 shadow-[var(--shadow-elevation-2)]"
          style={{
            backgroundColor: "var(--overlay-panel-bg)",
            borderColor: "var(--overlay-panel-border)",
            color: "var(--overlay-panel-fg)",
            boxShadow: "var(--shadow-elevation-2)",
          }}
          aria-hidden="true"
        >
          <div className="editor-empty-playback-pill h-7 w-[76px] rounded-xl bg-[var(--ui-surface-2)]" />
          <div className="editor-empty-playback-divider h-5 w-px bg-[var(--ui-border)]" />
          <div className="editor-empty-playback-icon h-8 w-8 rounded-lg bg-[var(--ui-surface-2)]" />
          <div className="editor-empty-playback-time h-4 w-[88px] rounded-full bg-[var(--ui-surface-2)]" />
          <div className="editor-empty-playback-divider h-5 w-px bg-[var(--ui-border)]" />
          <div className="editor-empty-playback-pill h-7 w-[94px] rounded-xl bg-[var(--ui-surface-2)]" />
          <div className="editor-empty-playback-pill h-7 w-[92px] rounded-xl bg-[var(--ui-surface-2)]" />
          <div className="editor-empty-playback-pill h-7 w-[108px] rounded-xl bg-[var(--ui-surface-2)]" />
        </div>
      )}
    </div>
  );
}
