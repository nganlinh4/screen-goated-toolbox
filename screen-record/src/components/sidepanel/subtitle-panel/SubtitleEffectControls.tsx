import { SettingRow } from '@/components/layout/SettingRow';
import { ColorPicker } from '@/components/ui/ColorPicker';
import { PanelSelect } from '@/components/ui/PanelSelect';
import { Slider } from '@/components/ui/Slider';
import { Checkbox } from '@/components/ui/checkbox';
import type { Translations } from '@/i18n';
import type { SubtitleSegment, TextAnimationPreset, TextStyle } from '@/types/video';

type SubtitleUpdater = (subtitle: SubtitleSegment) => SubtitleSegment;

interface SubtitleEffectControlsProps {
  t: Translations;
  resolvedStyle: TextStyle;
  onUpdateSelectedSubtitles: (updater: SubtitleUpdater) => void;
  beginBatch: () => void;
  commitBatch: () => void;
}

export function SubtitleEffectControls({
  t,
  resolvedStyle,
  onUpdateSelectedSubtitles,
  beginBatch,
  commitBatch,
}: SubtitleEffectControlsProps) {
  return (
    <>
      <div className="subtitle-animation-row flex items-center gap-3">
        <span className="w-20 flex-shrink-0 text-[11px] font-medium text-on-surface-variant">
          {t.animation}
        </span>
        <PanelSelect
          value={resolvedStyle.animation?.preset ?? 'fade'}
          options={[
            { value: 'none', label: t.animationPresetNone },
            { value: 'fade', label: t.animationPresetFade },
            { value: 'slide-up', label: t.animationPresetSlideUp },
            { value: 'pop', label: t.animationPresetPop },
          ]}
          onChange={(value) => onUpdateSelectedSubtitles((subtitle) => ({
            ...subtitle,
            style: {
              ...subtitle.style,
              animation: {
                ...(resolvedStyle.animation ?? { preset: 'fade', inDuration: 0.3, outDuration: 0.3 }),
                preset: value as TextAnimationPreset,
              },
            },
          }))}
          triggerClassName="subtitle-animation-select h-8 flex-1 rounded-lg px-2.5 text-[11px]"
          contentClassName="subtitle-animation-menu"
        />
      </div>

      {(resolvedStyle.animation?.preset ?? 'fade') !== 'none' ? (
        <div className="subtitle-animation-controls space-y-3.5 pl-1">
          <SettingRow label={t.animationInDuration} valueDisplay={`${(resolvedStyle.animation?.inDuration ?? 0.3).toFixed(2)}s`}>
            <Slider
              min={0.05}
              max={1.5}
              step={0.01}
              value={resolvedStyle.animation?.inDuration ?? 0.3}
              onPointerDown={beginBatch}
              onPointerUp={commitBatch}
              onChange={(value) => onUpdateSelectedSubtitles((subtitle) => ({
                ...subtitle,
                style: {
                  ...subtitle.style,
                  animation: { ...(resolvedStyle.animation ?? { preset: 'fade', inDuration: 0.3, outDuration: 0.3 }), inDuration: value },
                },
              }))}
            />
          </SettingRow>
          <SettingRow label={t.animationOutDuration} valueDisplay={`${(resolvedStyle.animation?.outDuration ?? 0.3).toFixed(2)}s`}>
            <Slider
              min={0.05}
              max={1.5}
              step={0.01}
              value={resolvedStyle.animation?.outDuration ?? 0.3}
              onPointerDown={beginBatch}
              onPointerUp={commitBatch}
              onChange={(value) => onUpdateSelectedSubtitles((subtitle) => ({
                ...subtitle,
                style: {
                  ...subtitle.style,
                  animation: { ...(resolvedStyle.animation ?? { preset: 'fade', inDuration: 0.3, outDuration: 0.3 }), outDuration: value },
                },
              }))}
            />
          </SettingRow>
        </div>
      ) : null}

      <label className="subtitle-background-toggle flex items-center gap-3 text-[11px] text-on-surface-variant cursor-pointer">
        <Checkbox
          checked={resolvedStyle.background?.enabled ?? false}
          onChange={(event) => onUpdateSelectedSubtitles((subtitle) => ({
            ...subtitle,
            style: {
              ...subtitle.style,
              background: {
                enabled: event.target.checked,
                color: resolvedStyle.background?.color ?? '#000000',
                opacity: resolvedStyle.background?.opacity ?? 0.65,
                paddingX: resolvedStyle.background?.paddingX ?? 16,
                paddingY: resolvedStyle.background?.paddingY ?? 8,
                borderRadius: resolvedStyle.background?.borderRadius ?? 32,
              },
            },
          }))}
        />
        {t.backgroundPill}
      </label>
      {resolvedStyle.background?.enabled ? (
        <div className="subtitle-background-controls mt-1 space-y-3.5 pl-1">
          <div className="subtitle-background-color-row flex items-center gap-3">
            <span className="w-20 flex-shrink-0 text-[11px] font-medium text-on-surface-variant">
              {t.pillColor}
            </span>
            <ColorPicker
              value={resolvedStyle.background.color.startsWith('rgba') ? '#000000' : resolvedStyle.background.color}
              onChange={(color) => onUpdateSelectedSubtitles((subtitle) => ({
                ...subtitle,
                style: {
                  ...subtitle.style,
                  background: { ...(resolvedStyle.background ?? subtitle.style.background)!, color },
                },
              }))}
              onOpen={beginBatch}
              onClose={commitBatch}
            />
          </div>

          <SettingRow
            label={t.pillOpacity}
            valueDisplay={`${Math.round((resolvedStyle.background.opacity ?? 0.65) * 100)}%`}
            className="subtitle-background-opacity-field"
          >
            <Slider
              min={0}
              max={1}
              step={0.01}
              value={resolvedStyle.background.opacity ?? 0.65}
              onPointerDown={beginBatch}
              onPointerUp={commitBatch}
              onChange={(value) => onUpdateSelectedSubtitles((subtitle) => ({
                ...subtitle,
                style: {
                  ...subtitle.style,
                  background: { ...(resolvedStyle.background ?? subtitle.style.background)!, opacity: value },
                },
              }))}
            />
          </SettingRow>
          <SettingRow
            label={t.paddingX}
            valueDisplay={`${resolvedStyle.background.paddingX}`}
            className="subtitle-background-padding-x-field"
          >
            <Slider
              min={0}
              max={64}
              step={1}
              value={resolvedStyle.background.paddingX}
              onPointerDown={beginBatch}
              onPointerUp={commitBatch}
              onChange={(value) => onUpdateSelectedSubtitles((subtitle) => ({
                ...subtitle,
                style: {
                  ...subtitle.style,
                  background: { ...(resolvedStyle.background ?? subtitle.style.background)!, paddingX: value },
                },
              }))}
            />
          </SettingRow>
          <SettingRow
            label={t.paddingY}
            valueDisplay={`${resolvedStyle.background.paddingY}`}
            className="subtitle-background-padding-y-field"
          >
            <Slider
              min={0}
              max={48}
              step={1}
              value={resolvedStyle.background.paddingY}
              onPointerDown={beginBatch}
              onPointerUp={commitBatch}
              onChange={(value) => onUpdateSelectedSubtitles((subtitle) => ({
                ...subtitle,
                style: {
                  ...subtitle.style,
                  background: { ...(resolvedStyle.background ?? subtitle.style.background)!, paddingY: value },
                },
              }))}
            />
          </SettingRow>

          <SettingRow
            label={t.pillRadius}
            valueDisplay={`${resolvedStyle.background.borderRadius}`}
            className="subtitle-background-radius-field"
          >
            <Slider
              min={0}
              max={32}
              step={1}
              value={resolvedStyle.background.borderRadius}
              onPointerDown={beginBatch}
              onPointerUp={commitBatch}
              onChange={(value) => onUpdateSelectedSubtitles((subtitle) => ({
                ...subtitle,
                style: {
                  ...subtitle.style,
                  background: { ...(resolvedStyle.background ?? subtitle.style.background)!, borderRadius: value },
                },
              }))}
            />
          </SettingRow>
        </div>
      ) : null}

      <div>
        <label className="subtitle-stroke-toggle flex items-center gap-3 text-[11px] text-on-surface-variant cursor-pointer">
          <Checkbox
            checked={resolvedStyle.stroke?.enabled ?? false}
            onChange={(event) => onUpdateSelectedSubtitles((subtitle) => ({
              ...subtitle,
              style: {
                ...subtitle.style,
                stroke: { ...(resolvedStyle.stroke ?? { enabled: false, color: '#000000', width: 2, opacity: 1 }), enabled: event.target.checked },
              },
            }))}
          />
          {t.stroke}
        </label>
        {resolvedStyle.stroke?.enabled ? (
          <div className="subtitle-stroke-controls mt-1 space-y-3.5 pl-1">
            <div className="subtitle-stroke-color-row flex items-center gap-3">
              <span className="w-20 flex-shrink-0 text-[11px] font-medium text-on-surface-variant">{t.strokeColor}</span>
              <ColorPicker
                value={resolvedStyle.stroke.color}
                onChange={(color) => onUpdateSelectedSubtitles((subtitle) => ({
                  ...subtitle,
                  style: {
                    ...subtitle.style,
                    stroke: { ...(resolvedStyle.stroke ?? subtitle.style.stroke)!, color },
                  },
                }))}
                onOpen={beginBatch}
                onClose={commitBatch}
              />
            </div>
            <SettingRow label={t.strokeWidth} valueDisplay={`${resolvedStyle.stroke.width}`}>
              <Slider
                min={0}
                max={16}
                step={0.5}
                value={resolvedStyle.stroke.width}
                onPointerDown={beginBatch}
                onPointerUp={commitBatch}
                onChange={(value) => onUpdateSelectedSubtitles((subtitle) => ({
                  ...subtitle,
                  style: {
                    ...subtitle.style,
                    stroke: { ...(resolvedStyle.stroke ?? subtitle.style.stroke)!, width: value },
                  },
                }))}
              />
            </SettingRow>
            <SettingRow label={t.strokeOpacity} valueDisplay={`${Math.round((resolvedStyle.stroke.opacity ?? 1) * 100)}%`}>
              <Slider
                min={0}
                max={1}
                step={0.01}
                value={resolvedStyle.stroke.opacity ?? 1}
                onPointerDown={beginBatch}
                onPointerUp={commitBatch}
                onChange={(value) => onUpdateSelectedSubtitles((subtitle) => ({
                  ...subtitle,
                  style: {
                    ...subtitle.style,
                    stroke: { ...(resolvedStyle.stroke ?? subtitle.style.stroke)!, opacity: value },
                  },
                }))}
              />
            </SettingRow>
          </div>
        ) : null}
      </div>

      <div>
        <label className="subtitle-shadow-toggle flex items-center gap-3 text-[11px] text-on-surface-variant cursor-pointer">
          <Checkbox
            checked={resolvedStyle.shadow?.enabled ?? true}
            onChange={(event) => onUpdateSelectedSubtitles((subtitle) => ({
              ...subtitle,
              style: {
                ...subtitle.style,
                shadow: { ...(resolvedStyle.shadow ?? { enabled: true, color: '#000000', blur: 4, offsetX: 2, offsetY: 2, opacity: 0.7 }), enabled: event.target.checked },
              },
            }))}
          />
          {t.shadow}
        </label>
        {resolvedStyle.shadow?.enabled ? (
          <div className="subtitle-shadow-controls mt-1 space-y-3.5 pl-1">
            <div className="subtitle-shadow-color-row flex items-center gap-3">
              <span className="w-20 flex-shrink-0 text-[11px] font-medium text-on-surface-variant">{t.shadowColor}</span>
              <ColorPicker
                value={resolvedStyle.shadow.color}
                onChange={(color) => onUpdateSelectedSubtitles((subtitle) => ({
                  ...subtitle,
                  style: {
                    ...subtitle.style,
                    shadow: { ...(resolvedStyle.shadow ?? subtitle.style.shadow)!, color },
                  },
                }))}
                onOpen={beginBatch}
                onClose={commitBatch}
              />
            </div>
            <SettingRow label={t.shadowBlur} valueDisplay={`${resolvedStyle.shadow.blur}`}>
              <Slider
                min={0}
                max={32}
                step={1}
                value={resolvedStyle.shadow.blur}
                onPointerDown={beginBatch}
                onPointerUp={commitBatch}
                onChange={(value) => onUpdateSelectedSubtitles((subtitle) => ({
                  ...subtitle,
                  style: {
                    ...subtitle.style,
                    shadow: { ...(resolvedStyle.shadow ?? subtitle.style.shadow)!, blur: value },
                  },
                }))}
              />
            </SettingRow>
            <SettingRow label={t.shadowOffsetX} valueDisplay={`${resolvedStyle.shadow.offsetX}`}>
              <Slider
                min={-24}
                max={24}
                step={1}
                value={resolvedStyle.shadow.offsetX}
                onPointerDown={beginBatch}
                onPointerUp={commitBatch}
                onChange={(value) => onUpdateSelectedSubtitles((subtitle) => ({
                  ...subtitle,
                  style: {
                    ...subtitle.style,
                    shadow: { ...(resolvedStyle.shadow ?? subtitle.style.shadow)!, offsetX: value },
                  },
                }))}
              />
            </SettingRow>
            <SettingRow label={t.shadowOffsetY} valueDisplay={`${resolvedStyle.shadow.offsetY}`}>
              <Slider
                min={-24}
                max={24}
                step={1}
                value={resolvedStyle.shadow.offsetY}
                onPointerDown={beginBatch}
                onPointerUp={commitBatch}
                onChange={(value) => onUpdateSelectedSubtitles((subtitle) => ({
                  ...subtitle,
                  style: {
                    ...subtitle.style,
                    shadow: { ...(resolvedStyle.shadow ?? subtitle.style.shadow)!, offsetY: value },
                  },
                }))}
              />
            </SettingRow>
            <SettingRow label={t.shadowOpacity} valueDisplay={`${Math.round((resolvedStyle.shadow.opacity ?? 0.7) * 100)}%`}>
              <Slider
                min={0}
                max={1}
                step={0.01}
                value={resolvedStyle.shadow.opacity ?? 0.7}
                onPointerDown={beginBatch}
                onPointerUp={commitBatch}
                onChange={(value) => onUpdateSelectedSubtitles((subtitle) => ({
                  ...subtitle,
                  style: {
                    ...subtitle.style,
                    shadow: { ...(resolvedStyle.shadow ?? subtitle.style.shadow)!, opacity: value },
                  },
                }))}
              />
            </SettingRow>
          </div>
        ) : null}
      </div>
    </>
  );
}
