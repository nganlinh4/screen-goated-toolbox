import type { Dispatch, ReactNode, SetStateAction } from "react";
import { AudioLines, Download, Plus } from '@/components/ui/MaterialIcon';
import type {
  AudioDownloadTrackKind,
  ImportedAudioSegment,
  NarrationSegment,
  VideoSegment,
} from "@/types/video";
import type { Translations } from "@/i18n";
import { Slider } from "@/components/ui/Slider";
import { Switch } from "@/components/ui/Switch";
import {
  normalizeTrackDelaySec,
  TRACK_DELAY_LIMIT_SEC,
} from "@/hooks/videoStatePreferences";

interface TimelineLabelColumnProps {
  t: Translations;
  segment: VideoSegment | null;
  duration: number;
  showZoom: boolean;
  showDebug: boolean;
  setShowDebug: Dispatch<SetStateAction<boolean>>;
  showSpeed: boolean;
  showImportedAudio: boolean;
  showDeviceAudio: boolean;
  showMicAudio: boolean;
  showWebcam: boolean;
  showNarration: boolean;
  showKeystroke: boolean;
  showPointer: boolean;
  showTrimLane: boolean;
  keystrokeTrackLabel: string;
  audioSegments?: ImportedAudioSegment[];
  narrationSegments?: NarrationSegment[];
  onTriggerImportedAudioPicker: () => void;
  onTriggerSubtitlePicker: () => void;
  canPickImportedAudioFile: boolean;
  canPickSubtitleFile: boolean;
  onAudioTrackDownload?: (trackKind: AudioDownloadTrackKind, trackLabel: string) => void;
  currentRawMicAudioPath: string;
  isMicAudioAvailable: boolean;
  isWebcamAvailable: boolean;
  volumeViewEnabled: boolean;
  setVolumeViewEnabled: (enabled: boolean) => void;
  setSegment: (segment: VideoSegment | null) => void;
  beginBatch: () => void;
  commitBatch: () => void;
}

export function TimelineLabelColumn({
  t,
  segment,
  duration,
  showZoom,
  showDebug,
  setShowDebug,
  showSpeed,
  showImportedAudio,
  showDeviceAudio,
  showMicAudio,
  showWebcam,
  showNarration,
  showKeystroke,
  showPointer,
  showTrimLane,
  keystrokeTrackLabel,
  audioSegments,
  narrationSegments,
  onTriggerImportedAudioPicker,
  onTriggerSubtitlePicker,
  canPickImportedAudioFile,
  canPickSubtitleFile,
  onAudioTrackDownload,
  currentRawMicAudioPath,
  isMicAudioAvailable,
  isWebcamAvailable,
  volumeViewEnabled,
  setVolumeViewEnabled,
  setSegment,
  beginBatch,
  commitBatch,
}: TimelineLabelColumnProps) {
  const renderDownloadButton = (
    trackKind: AudioDownloadTrackKind,
    label: string,
    groupName: string,
    disabled = false,
    offsetIndex = 0,
  ) => {
    if (disabled || !onAudioTrackDownload) return null;
    const hoverClass =
      groupName === "imported-audio-label"
        ? "group-hover/imported-audio-label:opacity-100"
        : groupName === "device-audio-label"
          ? "group-hover/device-audio-label:opacity-100"
          : groupName === "mic-audio-label"
            ? "group-hover/mic-audio-label:opacity-100"
            : "group-hover/narration-label:opacity-100";
    return (
      <button
        type="button"
        onClick={() => onAudioTrackDownload(trackKind, label)}
        className={`timeline-label-audio-download ui-icon-button absolute left-full top-1/2 z-20 h-5 w-5 -translate-y-1/2 rounded-full bg-[var(--surface)]/95 text-[var(--primary-color)] opacity-0 shadow-xs transition-opacity duration-150 ${hoverClass} focus-visible:opacity-100`}
        style={{ marginLeft: `${4 + offsetIndex * 24}px` }}
        title={t.downloadAudioTrack}
        aria-label={`${t.downloadAudioTrack}: ${label}`}
      >
        <Download className="h-3 w-3" strokeWidth={2.6} />
      </button>
    );
  };

  const renderTrackDelayLabel = ({
    className,
    groupClassName,
    label,
    value,
    onChange,
    isAvailable,
    heightClassName,
    action,
  }: {
    className: string;
    groupClassName: string;
    label: string;
    value: number;
    onChange: (value: number) => void;
    isAvailable: boolean;
    heightClassName: string;
    action?: ReactNode;
  }) => {
    const delayOffsetPx = action ? 28 : 8;
    return (
      <div
        className={`${className} ${heightClassName} relative flex items-center ${
          isAvailable ? "" : "timeline-label-unavailable"
        } ${groupClassName}`}
      >
        <div
          aria-hidden="true"
          className="timeline-label-track-delay-hover-bridge absolute left-full top-1/2 z-10 h-14 -translate-y-1/2 bg-transparent pointer-events-none group-hover:pointer-events-auto group-focus-within:pointer-events-auto"
          style={{ width: `${delayOffsetPx + 12}px` }}
        />
        <span className="text-[10px] font-semibold text-[var(--on-surface-variant)] leading-none">
          {label}
        </span>
        {action}
        <div
          className={`${className}-delay-popover timeline-label-track-delay-popover playback-keystroke-delay-popover absolute left-full top-1/2 z-30 -translate-y-1/2 w-[218px] px-2.5 py-2 rounded-lg border pointer-events-none opacity-0 translate-x-1 transition-all duration-150 group-hover:opacity-100 group-hover:translate-x-0 group-hover:pointer-events-auto group-focus-within:opacity-100 group-focus-within:translate-x-0 group-focus-within:pointer-events-auto`}
          style={{ marginLeft: `${delayOffsetPx}px` }}
        >
          <div className="flex items-center gap-3">
            <div className="flex-1 rounded-full px-1 py-[3px]">
              <Slider
                min={-TRACK_DELAY_LIMIT_SEC}
                max={TRACK_DELAY_LIMIT_SEC}
                step={0.01}
                value={value}
                disabled={!isAvailable || !segment}
                onPointerDown={beginBatch}
                onPointerUp={commitBatch}
                onPointerCancel={commitBatch}
                onChange={(nextValue) => onChange(normalizeTrackDelaySec(nextValue))}
                className={`${className}-delay-slider timeline-label-track-delay-slider playback-keystroke-delay-slider block w-full`}
              />
            </div>
            <span className="text-[10px] tabular-nums text-[var(--overlay-panel-fg)]/86 w-12 text-right">
              {value.toFixed(2)}s
            </span>
          </div>
        </div>
      </div>
    );
  };

  return (
    <div className="timeline-side-column w-[4rem] shrink-0">
      <div className="timeline-label-gutter flex flex-col gap-[2px] border-r border-[var(--ui-border)] pr-2">
        {showZoom && (
          <div className="timeline-label-zoom h-7 flex items-center justify-between">
            <span className="text-[10px] font-semibold text-[var(--on-surface-variant)] leading-none">
              {t.trackZoom}
            </span>
            <button
              onClick={() => setShowDebug((value) => !value)}
              className={`timeline-debug-btn w-3 h-3 rounded-xs text-[7px] font-bold leading-none flex items-center justify-center transition-colors ${
                showDebug
                  ? "bg-blue-500 text-white"
                  : "ui-surface text-[var(--outline)] hover:text-[var(--on-surface)]"
              }`}
              title="Debug zoom curve"
            >
              D
            </button>
          </div>
        )}
        {showZoom && showDebug && (
          <div className="timeline-label-debug h-7 flex items-center">
            <span className="text-[10px] font-semibold text-[var(--on-surface-variant)] leading-none opacity-50">
              dbg
            </span>
          </div>
        )}
        {showSpeed && (
          <div className="timeline-label-speed h-7 flex items-center justify-between">
            <span className="text-[10px] font-semibold text-[var(--on-surface-variant)] leading-none">
              {t.trackSpeed || "Speed"}
            </span>
            <button
              onClick={() => {
                if (!segment) return;
                beginBatch();
                setSegment({
                  ...segment,
                  speedPoints: [
                    { time: 0, speed: 1 },
                    { time: duration, speed: 1 },
                  ],
                });
                commitBatch();
              }}
              disabled={!segment}
              className="timeline-speed-reset-btn ui-icon-button p-1 text-[9px] font-mono leading-none disabled:opacity-40 disabled:hover:text-[var(--outline)] disabled:hover:bg-transparent"
              title={t.resetSpeed || "Reset"}
            >
              R
            </button>
          </div>
        )}
        {showImportedAudio && (
          <div className="timeline-label-imported-audio group/imported-audio-label relative h-7 flex items-center">
            <span className="text-[10px] font-semibold text-[var(--on-surface-variant)] leading-none">
              {t.trackAudio}
            </span>
            {renderDownloadButton("imported", t.trackAudio, "imported-audio-label", (audioSegments?.length ?? 0) === 0, canPickImportedAudioFile ? 1 : 0)}
            {canPickImportedAudioFile && (
              <button
                type="button"
                onClick={onTriggerImportedAudioPicker}
                className="timeline-label-imported-audio-add ui-icon-button absolute left-full ml-1 top-1/2 z-20 h-5 w-5 -translate-y-1/2 rounded-full bg-[var(--surface)]/95 text-[var(--primary-color)] opacity-0 shadow-xs transition-opacity duration-150 group-hover/imported-audio-label:opacity-100 focus-visible:opacity-100"
                title={t.addAudioFile}
                aria-label={t.addAudioFile}
              >
                <Plus className="h-3 w-3" strokeWidth={3} />
              </button>
            )}
          </div>
        )}
        {showDeviceAudio && renderTrackDelayLabel({
          className: "timeline-label-device-audio",
          groupClassName: "group group/device-audio-label",
          label: t.trackDeviceAudio,
          value: segment?.deviceAudioOffsetSec ?? 0,
          onChange: (value) => {
            if (!segment || segment.deviceAudioAvailable === false) return;
            setSegment({ ...segment, deviceAudioOffsetSec: value });
          },
          isAvailable: Boolean(segment && segment.deviceAudioAvailable !== false),
          heightClassName: "h-7",
          action: renderDownloadButton("device", t.trackDeviceAudio, "device-audio-label", !segment || segment.deviceAudioAvailable === false),
        })}
        {showMicAudio && renderTrackDelayLabel({
          className: "timeline-label-mic-audio",
          groupClassName: "group group/mic-audio-label",
          label: t.trackMicAudio,
          value: segment?.micAudioOffsetSec ?? 0,
          onChange: (value) => {
            if (!segment || !isMicAudioAvailable) return;
            setSegment({ ...segment, micAudioOffsetSec: value });
          },
          isAvailable: Boolean(segment && isMicAudioAvailable),
          heightClassName: "h-7",
          action: renderDownloadButton("mic", t.trackMicAudio, "mic-audio-label", !segment || !currentRawMicAudioPath.trim()),
        })}
        {showWebcam && renderTrackDelayLabel({
          className: "timeline-label-webcam",
          groupClassName: "group",
          label: t.trackWebcam,
          value: segment?.webcamOffsetSec ?? 0,
          onChange: (value) => {
            if (!segment || !isWebcamAvailable) return;
            setSegment({ ...segment, webcamOffsetSec: value });
          },
          isAvailable: Boolean(segment && isWebcamAvailable),
          heightClassName: "h-7",
        })}
        {showNarration && (
          <div className="timeline-label-narration group/narration-label relative h-7 flex items-center">
            <span className="text-[10px] font-semibold text-[var(--on-surface-variant)] leading-none">
              {t.tabNarration || "Narration"}
            </span>
            {renderDownloadButton("narration", t.tabNarration || "Narration", "narration-label", (narrationSegments?.length ?? 0) === 0)}
          </div>
        )}
        <div className="timeline-label-subtitles group/subtitle-label relative h-7 flex items-center">
          <span className="text-[10px] font-semibold text-[var(--on-surface-variant)] leading-none">
            {t.trackSubtitles}
          </span>
          {canPickSubtitleFile && (
            <button
              type="button"
              onClick={onTriggerSubtitlePicker}
              className="timeline-label-subtitles-add ui-icon-button absolute left-full ml-1 top-1/2 z-20 h-5 w-5 -translate-y-1/2 rounded-full bg-[var(--surface)]/95 text-[var(--primary-color)] opacity-0 shadow-xs transition-opacity duration-150 group-hover/subtitle-label:opacity-100 focus-visible:opacity-100"
              title={t.importSubtitleSrt}
              aria-label={t.importSubtitleSrt}
            >
              <Plus className="h-3 w-3" strokeWidth={3} />
            </button>
          )}
        </div>
        <div className="timeline-label-text h-7 flex items-center">
          <span className="text-[10px] font-semibold text-[var(--on-surface-variant)] leading-none">
            {t.trackText}
          </span>
        </div>
        {showKeystroke && (
          <div className="timeline-label-keystrokes h-7 flex items-center">
            <span className="text-[10px] font-semibold text-[var(--on-surface-variant)] leading-none">
              {keystrokeTrackLabel}
            </span>
          </div>
        )}
        {showPointer && (
          <div className="timeline-label-pointer h-7 flex items-center">
            <span className="text-[10px] font-semibold text-[var(--on-surface-variant)] leading-none">
              {t.trackPointer}
            </span>
          </div>
        )}
        {showTrimLane && (
          <div className="timeline-label-video h-10 flex items-center">
            <span className="text-[10px] font-semibold text-[var(--on-surface-variant)] leading-none">
              {t.trackVideo}
            </span>
          </div>
        )}
      </div>
      <div className="timeline-ruler-spacer h-4 mt-0.5 flex items-center justify-end">
        <div
          className="timeline-volume-view-toggle flex items-center gap-1"
          title={t.volumeView}
          aria-label={t.volumeView}
        >
          <AudioLines
            className="timeline-volume-view-icon h-3 w-3 text-[var(--on-surface-variant)]"
            strokeWidth={2}
            aria-hidden="true"
          />
          <Switch
            checked={volumeViewEnabled}
            onCheckedChange={setVolumeViewEnabled}
          />
        </div>
      </div>
    </div>
  );
}
