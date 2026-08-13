import { useEffect, useRef, useState } from "react";
import { CanvasRatioIcon } from "@/components/CanvasRatioIcon";
import { Check, Lock } from "@/components/ui/MaterialIcon";
import { useSettings } from "@/hooks/useSettings";
import {
  CROP_ASPECT_RATIO_ORIENTATIONS,
  CROP_ASPECT_RATIO_PRESETS,
  COMMON_CROP_ASPECT_RATIO_PRESET_IDS,
  type CropAspectRatioPreset,
  type CropAspectRatioOrientation,
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
  const lockedPreset = CROP_ASPECT_RATIO_PRESETS.find(({ id }) => id === lockedPresetId);
  const activeLabel = lockedPreset?.label ?? (sourceIsActive ? t.cropOriginal : t.cropCustom);
  const activePresetRef = useRef<HTMLButtonElement>(null);
  const [showAllPresets, setShowAllPresets] = useState(false);
  const commonPresetIds = new Set<string>(COMMON_CROP_ASPECT_RATIO_PRESET_IDS);
  const commonPresets = COMMON_CROP_ASPECT_RATIO_PRESET_IDS
    .map((id) => CROP_ASPECT_RATIO_PRESETS.find((preset) => preset.id === id))
    .filter((preset): preset is CropAspectRatioPreset => preset !== undefined);
  const additionalPresets = CROP_ASPECT_RATIO_PRESETS.filter(({ id }) => !commonPresetIds.has(id));
  const orientationLabels: Record<CropAspectRatioOrientation, string> = {
    landscape: t.cropLandscape,
    square: t.cropSquare,
    portrait: t.cropPortrait,
  };

  useEffect(() => {
    activePresetRef.current?.scrollIntoView?.({ block: "nearest", inline: "nearest" });
  }, [lockedPresetId, sourceIsActive]);

  useEffect(() => {
    if (lockedPresetId && !commonPresetIds.has(lockedPresetId)) setShowAllPresets(true);
  }, [lockedPresetId]);

  const renderPreset = (preset: CropAspectRatioPreset) => {
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
        ref={isActive ? activePresetRef : undefined}
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
  };

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

      <div className="crop-ratio-preset-list" aria-label={t.cropAspectRatio}>
        <button
          ref={sourceIsActive ? activePresetRef : undefined}
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

        <section className="crop-ratio-preset-group" aria-label={t.cropCommonRatios}>
          <h3 className="crop-ratio-group-heading">{t.cropCommonRatios}</h3>
          <div className="crop-ratio-group-grid">
            {commonPresets.map(renderPreset)}
          </div>
        </section>

        <button
          type="button"
          className="crop-ratio-more-toggle"
          aria-expanded={showAllPresets}
          aria-label={`${showAllPresets ? t.cropFewerRatios : t.cropMoreRatios}, ${additionalPresets.length}`}
          onClick={() => setShowAllPresets((current) => !current)}
        >
          <span>{showAllPresets ? t.cropFewerRatios : t.cropMoreRatios}</span>
          <span className="crop-ratio-more-count">{additionalPresets.length}</span>
        </button>

        {showAllPresets && CROP_ASPECT_RATIO_ORIENTATIONS.map((orientation) => {
          const orientationPresets = additionalPresets.filter(
            (preset) => preset.orientation === orientation,
          );
          if (orientationPresets.length === 0) return null;
          return (
            <section
              key={orientation}
              className="crop-ratio-preset-group crop-ratio-additional-group"
              aria-label={orientationLabels[orientation]}
            >
              <h3 className="crop-ratio-group-heading">
                {orientationLabels[orientation]}
              </h3>
              <div className="crop-ratio-group-grid">
                {orientationPresets.map(renderPreset)}
              </div>
            </section>
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
