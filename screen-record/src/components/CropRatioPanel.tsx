import { CanvasRatioIcon } from "@/components/CanvasRatioIcon";
import { Check, Lock } from "@/components/ui/MaterialIcon";
import { useSettings } from "@/hooks/useSettings";
import {
  POPULAR_ASPECT_RATIO_PRESETS,
  type AspectRatioPresetId,
} from "@/lib/aspectRatioPresets";
import {
  getAspectRatioCrop,
  isSourceCrop,
} from "@/lib/cropAspectRatio";
import { resolveCodecAlignedCropGeometry } from "@/lib/videoGeometry";
import type { CropRect } from "@/types/video";

const DEFAULT_CROP: CropRect = { x: 0, y: 0, width: 1, height: 1 };

interface CropRatioPanelProps {
  sourceWidth: number;
  sourceHeight: number;
  crop: CropRect;
  lockedPresetId: AspectRatioPresetId | null;
  onCropChange: (crop: CropRect, lockedPresetId: AspectRatioPresetId | null) => void;
  onUnlockRatio: () => void;
}

export function CropRatioPanel({
  sourceWidth,
  sourceHeight,
  crop,
  lockedPresetId,
  onCropChange,
  onUnlockRatio,
}: CropRatioPanelProps) {
  const { t } = useSettings();
  const hasSource = sourceWidth > 0 && sourceHeight > 0;
  const sourceIsActive = lockedPresetId === null && isSourceCrop(sourceWidth, sourceHeight, crop);
  const cropGeometry = hasSource
    ? resolveCodecAlignedCropGeometry(sourceWidth, sourceHeight, crop)
    : null;
  const sourceGeometry = hasSource
    ? resolveCodecAlignedCropGeometry(sourceWidth, sourceHeight, DEFAULT_CROP)
    : null;
  const lockedPreset = POPULAR_ASPECT_RATIO_PRESETS.find(({ id }) => id === lockedPresetId);
  const activeLabel = lockedPreset?.label ?? (sourceIsActive ? t.cropOriginal : t.cropCustom);

  return (
    <aside className="crop-ratio-panel ui-surface-elevated" aria-label={t.cropAspectRatio}>
      <div className="crop-ratio-heading-row">
        <div>
          <h2 className="crop-ratio-heading">{t.cropAspectRatio}</h2>
          <p className="crop-ratio-hint">{t.cropAspectRatioHint}</p>
        </div>
        <div className="crop-ratio-lock-state">
          <span className="crop-ratio-active-label" data-locked={lockedPreset ? "true" : "false"}>
            {lockedPreset && <Lock className="crop-ratio-lock-icon" />}
            {activeLabel}
          </span>
          {lockedPreset && (
            <button
              type="button"
              className="crop-ratio-unlock-button"
              onClick={onUnlockRatio}
              aria-label={t.cropUnlockAspectRatio}
            >
              {t.cropFree}
            </button>
          )}
        </div>
      </div>

      <div className="crop-ratio-preset-list" role="group" aria-label={t.cropAspectRatio}>
        <button
          type="button"
          className="crop-ratio-preset crop-ratio-source-preset"
          data-active={sourceIsActive ? "true" : "false"}
          aria-pressed={sourceIsActive}
          aria-label={`${t.cropOriginal}, ${sourceGeometry?.width ?? 0}×${sourceGeometry?.height ?? 0}`}
          disabled={!hasSource}
          onClick={() => sourceGeometry && onCropChange(sourceGeometry.crop, null)}
        >
          <span className="crop-ratio-icon-box">
            <CanvasRatioIcon ratioWidth={sourceWidth || 16} ratioHeight={sourceHeight || 9} />
          </span>
          <span className="crop-ratio-preset-copy">
            <span className="crop-ratio-preset-name">{t.cropOriginal}</span>
            <span className="crop-ratio-preset-size">
              {sourceGeometry ? `${sourceGeometry.width}×${sourceGeometry.height}` : "—"}
            </span>
          </span>
          {sourceIsActive && <Check className="crop-ratio-check" />}
        </button>

        {POPULAR_ASPECT_RATIO_PRESETS.map((preset) => {
          const presetCrop = hasSource
            ? getAspectRatioCrop(
                sourceWidth,
                sourceHeight,
                preset.width,
                preset.height,
                crop,
              )
            : DEFAULT_CROP;
          const geometry = hasSource
            ? resolveCodecAlignedCropGeometry(sourceWidth, sourceHeight, presetCrop)
            : null;
          const isActive = lockedPresetId === preset.id;
          const dimensions = geometry ? `${geometry.width}×${geometry.height}` : "—";

          return (
            <button
              type="button"
              key={preset.id}
              className="crop-ratio-preset"
              data-active={isActive ? "true" : "false"}
              aria-pressed={isActive}
              aria-label={`${t.cropAspectRatio} ${preset.label}, ${dimensions}`}
              disabled={!hasSource}
              onClick={() => onCropChange(presetCrop, preset.id)}
            >
              <span className="crop-ratio-icon-box">
                <CanvasRatioIcon ratioWidth={preset.width} ratioHeight={preset.height} />
              </span>
              <span className="crop-ratio-preset-copy">
                <span className="crop-ratio-preset-name">{preset.label}</span>
                <span className="crop-ratio-preset-size">{dimensions}</span>
              </span>
              {isActive && <Check className="crop-ratio-check" />}
            </button>
          );
        })}
      </div>

      <div className="crop-ratio-selection-readout">
        <span className="crop-ratio-selection-label">{t.cropSelection}</span>
        <strong className="crop-ratio-selection-size">
          {cropGeometry ? `${cropGeometry.width} × ${cropGeometry.height}` : "—"}
        </strong>
      </div>
    </aside>
  );
}
