import { VideoSegment } from "@/types/video";
import { Button } from "@/components/ui/button";
import { Keyboard } from "lucide-react";
import { useSettings } from "@/hooks/useSettings";
import { DEFAULT_KEYSTROKE_DELAY_SEC } from "@/hooks/useKeystrokeOverlayEditor";
import { saveKeystrokeLanguage } from "@/hooks/useVideoState";
import { sv } from "@/lib/appUtils";

export interface KeystrokeToggleControlProps {
  segment: VideoSegment | null;
  setSegment: (s: VideoSegment) => void;
  handleToggleKeystrokeMode: () => void;
  handleKeystrokeDelayChange: (delay: number) => void;
}

export function KeystrokeToggleControl({
  segment,
  setSegment,
  handleToggleKeystrokeMode,
  handleKeystrokeDelayChange,
}: KeystrokeToggleControlProps) {
  const { t } = useSettings();
  const keystrokeMode = segment?.keystrokeMode ?? "off";

  return (
    <div className="playback-keystroke-control relative">
      <div className="playback-keystroke-delay-hover-bridge absolute left-0 right-0 bottom-full h-3" />
      <Button
        onClick={handleToggleKeystrokeMode}
        disabled={!segment}
        className={`playback-keystroke-toggle-btn ui-action-button h-7 text-[11px] transition-colors ${
          !segment
            ? "text-[var(--overlay-panel-fg)]/40 cursor-not-allowed"
            : keystrokeMode === "off"
              ? "ui-chip-button bg-transparent text-[var(--overlay-panel-fg)]/85 hover:text-[var(--overlay-panel-fg)]"
              : ""
        }`}
        data-tone="success"
        data-active={!segment || keystrokeMode === "off" ? "false" : "true"}
      >
        <Keyboard className="playback-keystroke-toggle-icon w-3.5 h-3.5 mr-1.5" />
        <span className="playback-keystroke-toggle-label">
          {keystrokeMode === "keyboard"
            ? t.keystrokeModeKeyboard
            : keystrokeMode === "keyboardMouse"
              ? t.keystrokeModeKeyboardMouse
              : t.keystrokeModeOff}
        </span>
      </Button>
      <div className="playback-keystroke-delay-popover absolute left-1/2 z-30 -translate-x-1/2 bottom-[calc(100%+4px)] w-[308px] px-2.5 py-2 rounded-lg border pointer-events-none opacity-0 translate-y-1 transition-all duration-150 group-hover/playback-keystroke:opacity-100 group-hover/playback-keystroke:translate-y-0 group-hover/playback-keystroke:pointer-events-auto group-focus-within/playback-keystroke:opacity-100 group-focus-within/playback-keystroke:translate-y-0 group-focus-within/playback-keystroke:pointer-events-auto">
        <div className="playback-keystroke-delay-row flex items-center gap-3">
          <div className="playback-keystroke-delay-slider-shell flex-1 rounded-full px-1 py-[3px]">
            <input
              type="range"
              min="-1"
              max="1"
              step="0.01"
              disabled={!segment}
              value={segment?.keystrokeDelaySec ?? DEFAULT_KEYSTROKE_DELAY_SEC}
              style={sv(
                segment?.keystrokeDelaySec ?? DEFAULT_KEYSTROKE_DELAY_SEC,
                -1,
                1,
              )}
              onChange={(e) => handleKeystrokeDelayChange(Number(e.target.value))}
              className="playback-keystroke-delay-slider block w-full"
            />
          </div>
          <span className="playback-keystroke-delay-value text-[10px] tabular-nums text-[var(--overlay-panel-fg)]/86 w-12 text-right">
            {(segment?.keystrokeDelaySec ?? DEFAULT_KEYSTROKE_DELAY_SEC).toFixed(2)}s
          </span>
        </div>
        <div className="playback-keystroke-language-row flex items-center gap-3 mt-2">
          <span className="playback-keystroke-language-label text-[10px] text-[var(--overlay-panel-fg)]/60 shrink-0">
            {t.keystrokeLanguageLabel}
          </span>
          <div className="playback-keystroke-language-toggle ui-segmented ml-auto flex-nowrap rounded-md overflow-hidden">
            {(["en", "ko", "vi", "es", "ja", "zh"] as const).map((lang) => (
              <button
                key={lang}
                className={`playback-keystroke-language-btn ui-segmented-button px-2 py-0.5 text-[10px] uppercase ${
                  (segment?.keystrokeLanguage ?? "en") === lang
                    ? "ui-segmented-button-active"
                    : "text-[var(--overlay-panel-fg)]/70"
                }`}
                onClick={() => {
                  if (!segment) return;
                  saveKeystrokeLanguage(lang);
                  setSegment({ ...segment, keystrokeLanguage: lang });
                }}
                disabled={!segment}
              >
                {lang}
              </button>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
