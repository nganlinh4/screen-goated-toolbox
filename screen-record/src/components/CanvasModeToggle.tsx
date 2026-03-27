import { BackgroundConfig } from "@/types/video";
import { CanvasRatioIcon } from "@/components/CanvasRatioIcon";
import { useSettings } from "@/hooks/useSettings";
import {
  POPULAR_CANVAS_RATIO_PRESETS,
  getCanvasRatioDimensions,
  isCanvasRatioPresetActive,
} from "@/lib/appUtils";

export interface CanvasModeToggleProps {
  backgroundConfig: BackgroundConfig;
  setBackgroundConfig: (
    update: BackgroundConfig | ((prev: BackgroundConfig) => BackgroundConfig),
  ) => void;
  customCanvasBaseDimensions: { width: number; height: number };
  getAutoCanvasSelectionConfig: () => {
    canvasMode: "auto";
    canvasWidth: number | undefined;
    canvasHeight: number | undefined;
    autoSourceClipId: string;
  };
  handleActivateCustomCanvas: () => void;
  handleApplyCanvasRatioPreset: (ratioWidth: number, ratioHeight: number) => void;
  isAutoCanvasDisabled?: boolean;
}

export function CanvasModeToggle({
  backgroundConfig,
  setBackgroundConfig,
  customCanvasBaseDimensions,
  getAutoCanvasSelectionConfig,
  handleActivateCustomCanvas,
  handleApplyCanvasRatioPreset,
  isAutoCanvasDisabled,
}: CanvasModeToggleProps) {
  const { t } = useSettings();
  const canvasMode = backgroundConfig.canvasMode ?? "auto";

  if (isAutoCanvasDisabled) {
    // Non-source clip in multi-clip: show locked canvas indicator, no editing
    return (
      <div className="playback-canvas-mode-toggle ui-segmented opacity-50 pointer-events-none" title={t.canvasCustom}>
        <button type="button" disabled className="playback-canvas-mode-btn ui-segmented-button ui-segmented-button-active px-2 py-1 text-[10px] font-semibold">
          {t.canvasCustom} {backgroundConfig.canvasWidth && backgroundConfig.canvasHeight ? `${backgroundConfig.canvasWidth}×${backgroundConfig.canvasHeight}` : ''}
        </button>
      </div>
    );
  }

  return (
    <div className="playback-canvas-mode-toggle ui-segmented">
      <button
        type="button"
        aria-pressed={canvasMode === "auto"}
        data-active={canvasMode === "auto" ? "true" : "false"}
        disabled={isAutoCanvasDisabled}
        onClick={() => {
          if (isAutoCanvasDisabled) return;
          const autoCanvasConfig = getAutoCanvasSelectionConfig();
          setBackgroundConfig((prev) => ({
            ...prev,
            canvasMode: "auto",
            canvasWidth: autoCanvasConfig.canvasWidth,
            canvasHeight: autoCanvasConfig.canvasHeight,
            autoCanvasSourceId: autoCanvasConfig.autoSourceClipId,
          }));
        }}
        className={`playback-canvas-mode-btn playback-canvas-mode-btn-auto ui-segmented-button ${
          isAutoCanvasDisabled
            ? "playback-canvas-mode-btn-disabled text-[var(--overlay-panel-fg)]/30 cursor-not-allowed"
            : canvasMode === "auto"
              ? "playback-canvas-mode-btn-active ui-segmented-button-active"
              : "playback-canvas-mode-btn-inactive text-[var(--overlay-panel-fg)]/70 hover:text-[var(--overlay-panel-fg)]"
        } px-2 py-1 text-[10px] font-semibold`}
      >
        {t.canvasAuto}
      </button>
      <div className="playback-canvas-custom-control relative group/playback-canvas-custom">
        <div className="playback-canvas-ratio-hover-bridge absolute left-0 right-0 bottom-full h-3" />
        <button
          type="button"
          aria-pressed={canvasMode === "custom"}
          data-active={canvasMode === "custom" ? "true" : "false"}
          onClick={handleActivateCustomCanvas}
          className={`playback-canvas-mode-btn playback-canvas-mode-btn-custom ui-segmented-button ${
            canvasMode === "custom"
              ? "playback-canvas-mode-btn-active ui-segmented-button-active"
              : "playback-canvas-mode-btn-inactive text-[var(--overlay-panel-fg)]/70 hover:text-[var(--overlay-panel-fg)]"
          } px-2 py-1 text-[10px] font-semibold`}
        >
          {t.canvasCustom}
        </button>
        <div className="playback-canvas-ratio-popover playback-keystroke-delay-popover absolute left-1/2 z-30 -translate-x-1/2 bottom-[calc(100%+4px)] w-[296px] px-2.5 py-2 rounded-lg border pointer-events-none opacity-0 translate-y-1 transition-all duration-150 group-hover/playback-canvas-custom:opacity-100 group-hover/playback-canvas-custom:translate-y-0 group-hover/playback-canvas-custom:pointer-events-auto group-focus-within/playback-canvas-custom:opacity-100 group-focus-within/playback-canvas-custom:translate-y-0 group-focus-within/playback-canvas-custom:pointer-events-auto">
          <div className="playback-canvas-ratio-grid grid grid-cols-3 gap-2">
            {POPULAR_CANVAS_RATIO_PRESETS.map((preset) => {
              const presetDimensions = getCanvasRatioDimensions(
                customCanvasBaseDimensions.width,
                customCanvasBaseDimensions.height,
                preset.width,
                preset.height,
              );
              const isActive =
                canvasMode === "custom" &&
                isCanvasRatioPresetActive(
                  backgroundConfig.canvasWidth,
                  backgroundConfig.canvasHeight,
                  preset.width,
                  preset.height,
                );

              return (
                <button
                  key={preset.id}
                  type="button"
                  onClick={() =>
                    handleApplyCanvasRatioPreset(preset.width, preset.height)
                  }
                  className={`playback-canvas-ratio-btn ui-choice-tile rounded-xl px-2.5 py-2 text-left ${
                    isActive
                      ? "ui-choice-tile-active"
                      : "text-[var(--overlay-panel-fg)]"
                  }`}
                >
                  <div className="playback-canvas-ratio-header flex items-center gap-2">
                    <CanvasRatioIcon
                      ratioWidth={preset.width}
                      ratioHeight={preset.height}
                    />
                    <span className="playback-canvas-ratio-label text-[10px] font-semibold">
                      {preset.label}
                    </span>
                  </div>
                  <div className="playback-canvas-ratio-size mt-1 text-[9px] tabular-nums text-[var(--overlay-panel-fg)]/68">
                    {presetDimensions.width}×{presetDimensions.height}
                  </div>
                </button>
              );
            })}
          </div>
        </div>
      </div>
    </div>
  );
}
