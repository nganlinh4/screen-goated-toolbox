import type { PointerEvent } from "react";
import { ChevronDown, Loader2, Mic, MonitorSpeaker, Volume2 } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuTrigger,
} from "@/components/ui/DropdownMenu";
import type { Translations } from "@/i18n";
import type { RecordingAudioSelection } from "@/types/recordingAudio";

interface RecordingAudioSourceDropdownProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onTriggerPointerDown: (e: PointerEvent<HTMLButtonElement>) => void;
  selection: RecordingAudioSelection;
  isSelectingApp: boolean;
  onToggleDevice: (enabled: boolean) => void;
  onToggleMic: (enabled: boolean) => void;
  onSelectAllDeviceAudio: () => void;
  onRequestAppSelection: () => void;
  t: Translations;
}

function buildSummaryLabel(
  selection: RecordingAudioSelection,
  t: Translations,
): string {
  if (selection.deviceEnabled && selection.micEnabled) {
    return `${t.trackDeviceAudio} + ${t.trackMicAudio}`;
  }
  if (selection.deviceEnabled) {
    if (selection.deviceMode === "app" && selection.selectedDeviceApp) {
      return selection.selectedDeviceApp.name;
    }
    return t.trackDeviceAudio;
  }
  if (selection.micEnabled) {
    return t.trackMicAudio;
  }
  return t.recordingAudioMuted;
}

export function RecordingAudioSourceDropdown({
  open,
  onOpenChange,
  onTriggerPointerDown,
  selection,
  isSelectingApp,
  onToggleDevice,
  onToggleMic,
  onSelectAllDeviceAudio,
  onRequestAppSelection,
  t,
}: RecordingAudioSourceDropdownProps) {
  const summaryLabel = buildSummaryLabel(selection, t);

  return (
    <div
      className="recording-audio-dropdown relative flex-shrink-0"
      onMouseDown={(e) => e.stopPropagation()}
    >
      <DropdownMenu open={open} onOpenChange={onOpenChange}>
        <DropdownMenuTrigger asChild>
          <Button
            onPointerDown={onTriggerPointerDown}
            className="recording-audio-toggle-btn ui-toolbar-button px-2 h-6 text-[11px] whitespace-nowrap flex items-center gap-1.5"
            title={t.recordingAudioSource}
          >
            <Volume2 className="w-3.5 h-3.5 flex-shrink-0" />
            <span className="recording-audio-toggle-label truncate">
              {summaryLabel}
            </span>
            <ChevronDown className="w-3 h-3 ml-0.5 flex-shrink-0" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent
          align="start"
          className="recording-audio-menu min-w-[300px]"
          onCloseAutoFocus={(e) => e.preventDefault()}
        >
          <div
            className="recording-audio-menu-body flex flex-col gap-1 p-1"
            onPointerDown={(e) => e.stopPropagation()}
          >
            <div className="recording-audio-menu-title px-2 pt-1 pb-0.5 text-[10px] uppercase tracking-wide text-[var(--on-surface-variant)] opacity-60">
              {t.recordingAudioSource}
            </div>

            <label className="recording-audio-toggle-row flex items-center gap-2 rounded-md px-2 py-2 text-[11px] text-[var(--on-surface)] hover:bg-[var(--ui-hover)] cursor-pointer">
              <Checkbox
                className="recording-audio-device-checkbox"
                checked={selection.deviceEnabled}
                onChange={(e) => onToggleDevice(e.target.checked)}
              />
              <MonitorSpeaker className="w-3.5 h-3.5 text-[var(--on-surface-variant)]" />
              <span className="recording-audio-device-label flex-1">
                {t.trackDeviceAudio}
              </span>
            </label>

            <label className="recording-audio-toggle-row flex items-center gap-2 rounded-md px-2 py-2 text-[11px] text-[var(--on-surface)] hover:bg-[var(--ui-hover)] cursor-pointer">
              <Checkbox
                className="recording-audio-mic-checkbox"
                checked={selection.micEnabled}
                onChange={(e) => onToggleMic(e.target.checked)}
              />
              <Mic className="w-3.5 h-3.5 text-[var(--on-surface-variant)]" />
              <span className="recording-audio-mic-label flex-1">
                {t.trackMicAudio}
              </span>
            </label>

            {selection.deviceEnabled && (
              <div className="recording-audio-device-mode-panel mt-1 rounded-lg border border-[var(--outline-variant)]/70 bg-[var(--ui-surface-2)]/75 p-2">
                <div className="recording-audio-device-mode-header mb-2 text-[10px] uppercase tracking-wide text-[var(--on-surface-variant)] opacity-60">
                  {t.trackDeviceAudio}
                </div>
                <div className="recording-audio-device-mode-options flex gap-1.5">
                  <button
                    type="button"
                    onClick={onSelectAllDeviceAudio}
                    className={`recording-audio-device-mode-btn flex-1 rounded-md px-2 py-1.5 text-[11px] transition-colors ${
                      selection.deviceMode === "all"
                        ? "bg-[color-mix(in_srgb,var(--primary-color)_14%,var(--ui-surface-3))] text-[var(--primary-color)]"
                        : "bg-[var(--ui-surface-3)] text-[var(--on-surface-variant)] hover:bg-[var(--ui-hover)]"
                    }`}
                  >
                    {t.recordingAudioAll}
                  </button>
                  <button
                    type="button"
                    onClick={onRequestAppSelection}
                    className={`recording-audio-device-mode-btn flex-1 rounded-md px-2 py-1.5 text-[11px] transition-colors ${
                      selection.deviceMode === "app"
                        ? "bg-[color-mix(in_srgb,var(--primary-color)_14%,var(--ui-surface-3))] text-[var(--primary-color)]"
                        : "bg-[var(--ui-surface-3)] text-[var(--on-surface-variant)] hover:bg-[var(--ui-hover)]"
                    }`}
                  >
                    {t.recordingAudioOneApp}
                  </button>
                </div>
                {selection.deviceMode === "app" && selection.selectedDeviceApp && (
                  <div className="recording-audio-selected-app-row mt-2 flex items-center gap-2 rounded-md bg-[var(--ui-surface-3)] px-2 py-1.5">
                    <div className="recording-audio-selected-app-name min-w-0 flex-1 truncate text-[11px] text-[var(--on-surface)]">
                      {selection.selectedDeviceApp.name}
                    </div>
                    <button
                      type="button"
                      onClick={onRequestAppSelection}
                      className="recording-audio-change-app-btn text-[10px] font-medium text-[var(--primary-color)] hover:opacity-80"
                    >
                      {t.recordingAudioChangeApp}
                    </button>
                  </div>
                )}
                {isSelectingApp && (
                  <div className="recording-audio-app-pending mt-2 flex items-center gap-1.5 text-[10px] text-[var(--on-surface-variant)]">
                    <Loader2 className="w-3 h-3 animate-spin" />
                    <span>{t.recordingAudioChoosingApp}</span>
                  </div>
                )}
              </div>
            )}
          </div>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}
