import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { PanelCard } from "@/components/layout/PanelCard";
import { SettingRow } from "@/components/layout/SettingRow";
import { Slider } from "@/components/ui/Slider";
import { useSettings } from "@/hooks/useSettings";
import type { WebcamConfig, WebcamPosition } from "@/types/video";

interface CameraPanelProps {
  webcamConfig: WebcamConfig;
  setWebcamConfig: React.Dispatch<React.SetStateAction<WebcamConfig>>;
  webcamAvailable: boolean;
  beginBatch: () => void;
  commitBatch: () => void;
}

const POSITION_OPTIONS: Array<{
  id: WebcamPosition;
  label: string;
}> = [
  { id: "topLeft", label: "TL" },
  { id: "topRight", label: "TR" },
  { id: "bottomLeft", label: "BL" },
  { id: "bottomRight", label: "BR" },
];

export function CameraPanel({
  webcamConfig,
  setWebcamConfig,
  webcamAvailable,
  beginBatch,
  commitBatch,
}: CameraPanelProps) {
  const { t } = useSettings();

  const updateConfig = (updates: Partial<WebcamConfig>) => {
    setWebcamConfig((prev) => ({
      ...prev,
      ...updates,
    }));
  };

  return (
    <PanelCard className="camera-panel">
      <div className="panel-header mb-3">
        <h2 className="panel-title text-xs font-medium uppercase tracking-wide text-on-surface-variant">
          {t.cameraSettings}
        </h2>
        {!webcamAvailable && (
          <p className="camera-panel-unavailable mt-2 text-[11px] text-on-surface-variant/80">
            {t.cameraUnavailable}
          </p>
        )}
      </div>

      <div
        className={`camera-panel-controls space-y-3.5 ${webcamAvailable ? "" : "opacity-45 pointer-events-none"}`}
      >
        <SettingRow
          label={t.cameraShow}
          valueDisplay={webcamConfig.visible ? t.rec : t.recordingAudioMuted}
          className="camera-show-field"
        >
          <label className="camera-toggle-row flex items-center justify-between gap-3 rounded-xl border border-[var(--ui-border)] bg-[var(--ui-surface-2)] px-3 py-2">
            <span className="text-[11px] text-on-surface-variant">
              {t.cameraShow}
            </span>
            <Checkbox
              checked={webcamConfig.visible}
              onChange={(event) =>
                updateConfig({ visible: event.currentTarget.checked })
              }
              disabled={!webcamAvailable}
            />
          </label>
        </SettingRow>

        <SettingRow label={t.cameraPosition} className="camera-position-field">
          <div className="camera-position-grid grid grid-cols-2 gap-2">
            {POSITION_OPTIONS.map((option) => {
              const isActive = webcamConfig.position === option.id;
              return (
                <Button
                  key={option.id}
                  type="button"
                  size="sm"
                  variant={isActive ? "default" : "outline"}
                  className="camera-position-button h-8"
                  onClick={() => updateConfig({ position: option.id })}
                  disabled={!webcamAvailable}
                >
                  {option.label}
                </Button>
              );
            })}
          </div>
        </SettingRow>

        <SettingRow label={t.cameraMirror} className="camera-mirror-field">
          <label className="camera-toggle-row flex items-center justify-between gap-3 rounded-xl border border-[var(--ui-border)] bg-[var(--ui-surface-2)] px-3 py-2">
            <span className="text-[11px] text-on-surface-variant">
              {t.cameraMirror}
            </span>
            <Checkbox
              checked={webcamConfig.mirror}
              onChange={(event) =>
                updateConfig({ mirror: event.currentTarget.checked })
              }
              disabled={!webcamAvailable}
            />
          </label>
        </SettingRow>

        <SettingRow
          label={t.cameraAutoSize}
          className="camera-auto-size-field"
        >
          <label className="camera-toggle-row flex items-center justify-between gap-3 rounded-xl border border-[var(--ui-border)] bg-[var(--ui-surface-2)] px-3 py-2">
            <span className="text-[11px] text-on-surface-variant">
              {t.cameraAutoSize}
            </span>
            <Checkbox
              checked={webcamConfig.autoSizeDuringZoom}
              onChange={(event) =>
                updateConfig({ autoSizeDuringZoom: event.currentTarget.checked })
              }
              disabled={!webcamAvailable}
            />
          </label>
        </SettingRow>

        <SettingRow
          label={t.cameraMaxSize}
          valueDisplay={`${Math.round(webcamConfig.maxSizePercent)}%`}
          className="camera-max-size-field"
        >
          <Slider
            min={10}
            max={34}
            step={1}
            value={webcamConfig.maxSizePercent}
            onPointerDown={beginBatch}
            onPointerUp={commitBatch}
            onChange={(value) =>
              updateConfig({
                maxSizePercent: Math.max(
                  value,
                  webcamConfig.autoSizeDuringZoom
                    ? webcamConfig.minSizePercent
                    : 10,
                ),
              })
            }
          />
        </SettingRow>

        <SettingRow
          label={t.cameraMinSize}
          valueDisplay={`${Math.round(webcamConfig.minSizePercent)}%`}
          className="camera-min-size-field"
        >
          <Slider
            min={8}
            max={28}
            step={1}
            value={webcamConfig.minSizePercent}
            onPointerDown={beginBatch}
            onPointerUp={commitBatch}
            onChange={(value) =>
              updateConfig({
                minSizePercent: Math.min(value, webcamConfig.maxSizePercent),
              })
            }
          />
        </SettingRow>

        <SettingRow
          label={t.roundness}
          valueDisplay={`${Math.round(webcamConfig.roundnessPx)}px`}
          className="camera-roundness-field"
        >
          <Slider
            min={0}
            max={80}
            step={1}
            value={webcamConfig.roundnessPx}
            onPointerDown={beginBatch}
            onPointerUp={commitBatch}
            onChange={(value) => updateConfig({ roundnessPx: value })}
          />
        </SettingRow>

        <SettingRow
          label={t.shadow}
          valueDisplay={`${Math.round(webcamConfig.shadowPx)}px`}
          className="camera-shadow-field"
        >
          <Slider
            min={0}
            max={48}
            step={1}
            value={webcamConfig.shadowPx}
            onPointerDown={beginBatch}
            onPointerUp={commitBatch}
            onChange={(value) => updateConfig({ shadowPx: value })}
          />
        </SettingRow>
      </div>
    </PanelCard>
  );
}
