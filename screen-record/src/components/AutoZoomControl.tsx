import { Wand2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { useSettings } from "@/hooks/useSettings";
import { sv } from "@/lib/appUtils";
import type { AutoZoomConfig, VideoSegment } from "@/types/video";

export interface AutoZoomControlProps {
  segment: VideoSegment | null;
  disabled: boolean;
  handleAutoZoom: () => void;
  autoZoomConfig: AutoZoomConfig;
  onConfigChange: (config: AutoZoomConfig) => void;
}

export function AutoZoomControl({
  segment,
  disabled,
  handleAutoZoom,
  autoZoomConfig,
  onConfigChange,
}: AutoZoomControlProps) {
  const { t } = useSettings();
  const isActive = !!(segment?.smoothMotionPath?.length);

  const buttonClass = `auto-zoom-button ui-action-button flex items-center px-2.5 py-1 h-7 text-xs font-medium transition-colors whitespace-nowrap rounded-lg ${
    disabled
      ? "ui-toolbar-button text-[var(--on-surface)]/35 cursor-not-allowed"
      : isActive
        ? ""
        : "ui-chip-button text-[var(--on-surface)]"
  }`;

  return (
    <div className="auto-zoom-control relative">
      <div className="auto-zoom-hover-bridge absolute left-0 right-0 bottom-full h-3" />
      <Button
        onClick={handleAutoZoom}
        disabled={disabled}
        className={buttonClass}
        data-tone="primary"
        data-active={isActive ? "true" : "false"}
      >
        <Wand2 className="w-3 h-3 mr-1" />
        {t.autoZoom}
      </Button>
      <div className="auto-zoom-popover absolute left-1/2 z-30 -translate-x-1/2 bottom-[calc(100%+4px)] w-[260px] px-3 py-2.5 rounded-lg border pointer-events-none opacity-0 translate-y-1 transition-all duration-150 group-hover/playback-auto-zoom:opacity-100 group-hover/playback-auto-zoom:translate-y-0 group-hover/playback-auto-zoom:pointer-events-auto group-focus-within/playback-auto-zoom:opacity-100 group-focus-within/playback-auto-zoom:translate-y-0 group-focus-within/playback-auto-zoom:pointer-events-auto">
        <SliderRow
          label={t.autoZoomFollowTightness}
          value={autoZoomConfig.followTightness}
          min={0}
          max={1}
          step={0.01}
          displayValue={`${Math.round(autoZoomConfig.followTightness * 100)}%`}
          disabled={disabled}
          onChange={(v) => onConfigChange({ ...autoZoomConfig, followTightness: v })}
        />
        <SliderRow
          label={t.autoZoomZoomLevel}
          value={autoZoomConfig.zoomLevel}
          min={1.2}
          max={4.0}
          step={0.1}
          displayValue={`${autoZoomConfig.zoomLevel.toFixed(1)}x`}
          disabled={disabled}
          onChange={(v) => onConfigChange({ ...autoZoomConfig, zoomLevel: v })}
        />
        <SliderRow
          label={t.autoZoomSpeedSensitivity}
          value={autoZoomConfig.speedSensitivity}
          min={0}
          max={1}
          step={0.01}
          displayValue={`${Math.round(autoZoomConfig.speedSensitivity * 100)}%`}
          disabled={disabled}
          onChange={(v) => onConfigChange({ ...autoZoomConfig, speedSensitivity: v })}
        />
      </div>
    </div>
  );
}

function SliderRow({
  label,
  value,
  min,
  max,
  step,
  displayValue,
  disabled,
  onChange,
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  displayValue: string;
  disabled: boolean;
  onChange: (v: number) => void;
}) {
  return (
    <div className="auto-zoom-slider-row flex items-center gap-2.5 mt-1.5 first:mt-0">
      <span className="auto-zoom-slider-label text-[10px] text-[var(--overlay-panel-fg)]/60 shrink-0 w-[72px]">
        {label}
      </span>
      <div className="auto-zoom-slider-shell flex-1 rounded-full px-1 py-[3px]">
        <input
          type="range"
          min={min}
          max={max}
          step={step}
          value={value}
          disabled={disabled}
          style={sv(value, min, max)}
          onChange={(e) => onChange(Number(e.target.value))}
          className="auto-zoom-slider block w-full"
        />
      </div>
      <span className="auto-zoom-slider-value text-[10px] tabular-nums text-[var(--overlay-panel-fg)]/86 w-10 text-right">
        {displayValue}
      </span>
    </div>
  );
}
