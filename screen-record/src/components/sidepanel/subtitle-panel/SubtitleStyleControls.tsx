import { AlignCenter, AlignLeft, AlignRight } from '@/components/ui/MaterialIcon';
import { SettingRow } from '@/components/layout/SettingRow';
import { ColorPicker } from '@/components/ui/ColorPicker';
import { Slider } from '@/components/ui/Slider';
import { Checkbox } from '@/components/ui/checkbox';
import { SmartSplitControl } from '@/components/sidepanel/SmartSplitControl';
import type { Translations } from '@/i18n';
import type { SubtitleSegment, TextStyle } from '@/types/video';
import { SubtitleEffectControls } from './SubtitleEffectControls';

type SubtitleUpdater = (subtitle: SubtitleSegment) => SubtitleSegment;

interface SubtitleStyleControlsProps {
  t: Translations;
  sourceSubtitle: SubtitleSegment;
  editableSubtitles: SubtitleSegment[];
  resolvedStyle: TextStyle;
  hasSelection: boolean;
  selectedSubtitleCount: number;
  isCustomSubtitleView: boolean;
  onUpdateSubtitleText: (text: string) => void;
  onUpdateSelectedSubtitles: (updater: SubtitleUpdater) => void;
  onSplitSelectedSubtitles: (maxUnits: number) => void;
  beginBatch: () => void;
  commitBatch: () => void;
}

function buildFontVariationCSS(vars?: TextStyle['fontVariations']): string | undefined {
  const parts: string[] = [];
  if (vars?.wdth !== undefined && vars.wdth !== 100) parts.push(`'wdth' ${vars.wdth}`);
  if (vars?.slnt !== undefined && vars.slnt !== 0) parts.push(`'slnt' ${vars.slnt}`);
  if (vars?.ROND !== undefined && vars.ROND !== 0) parts.push(`'ROND' ${vars.ROND}`);
  return parts.length > 0 ? parts.join(', ') : undefined;
}

export function SubtitleStyleControls({
  t,
  sourceSubtitle,
  editableSubtitles,
  resolvedStyle,
  hasSelection,
  selectedSubtitleCount,
  isCustomSubtitleView,
  onUpdateSubtitleText,
  onUpdateSelectedSubtitles,
  onSplitSelectedSubtitles,
  beginBatch,
  commitBatch,
}: SubtitleStyleControlsProps) {
  const isMultiSelect = selectedSubtitleCount >= 2;

  return (
    <div className="subtitle-style-controls space-y-3">
      <div className="subtitle-badge-row flex items-center gap-1.5">
        <div
          className="subtitle-preview-badge inline-flex items-center gap-1.5 rounded-full px-2.5 py-1 text-[10px] font-medium"
          style={{ background: 'color-mix(in srgb, var(--timeline-zoom-color) 15%, transparent)', color: 'var(--timeline-zoom-color)' }}
        >
          <AlignCenter className="h-3 w-3" />
          {hasSelection ? `${editableSubtitles.length} ${t.trackSubtitles}` : t.trackSubtitles}
        </div>

        {isMultiSelect ? (
          <div
            className="subtitle-multi-select-badge rounded-md px-2 py-1 text-[10px] font-medium"
            style={{ background: 'color-mix(in srgb, var(--timeline-zoom-color) 15%, transparent)', color: 'var(--timeline-zoom-color)' }}
          >
            {selectedSubtitleCount} {t.textMultiSelectLabel}
          </div>
        ) : null}
      </div>

      <textarea
        value={sourceSubtitle.text}
        onFocus={beginBatch}
        onBlur={commitBatch}
        onChange={(event) => onUpdateSubtitleText(event.target.value)}
        className="subtitle-editor-input ui-input w-full rounded-xl px-3 py-2 text-sm text-on-surface thin-scrollbar subtle-resize"
        disabled={isCustomSubtitleView}
        style={{
          fontFamily: "'Google Sans Flex', 'Segoe UI', system-ui, sans-serif",
          fontWeight: resolvedStyle.fontVariations?.wght ?? 400,
          fontVariationSettings: buildFontVariationCSS(resolvedStyle.fontVariations),
        }}
        rows={2}
      />

      <p className="text-[10px] text-on-surface-variant">
        {isCustomSubtitleView ? t.subtitleCustomReadOnly : t.dragTextHint}
      </p>

      <SmartSplitControl
        className="subtitle-smart-split-control"
        disabled={isCustomSubtitleView}
        targetCount={editableSubtitles.length}
        onSplit={onSplitSelectedSubtitles}
      />

      <SettingRow label={t.fontSize} valueDisplay={`${resolvedStyle.fontSize}`}>
        <Slider
          min={12}
          max={200}
          step={1}
          value={resolvedStyle.fontSize}
          onPointerDown={beginBatch}
          onPointerUp={commitBatch}
          onChange={(value) => onUpdateSelectedSubtitles((subtitle) => ({
            ...subtitle,
            style: { ...subtitle.style, fontSize: value },
          }))}
        />
      </SettingRow>

      <div className="subtitle-color-row flex items-center gap-3">
        <span className="w-20 flex-shrink-0 text-[11px] font-medium text-on-surface-variant">{t.color}</span>
        <ColorPicker
          value={resolvedStyle.color}
          onChange={(color) => onUpdateSelectedSubtitles((subtitle) => ({
            ...subtitle,
            style: { ...subtitle.style, color },
          }))}
          onOpen={beginBatch}
          onClose={commitBatch}
        />
      </div>

      {([
        { axis: 'wght', label: t.fontWeight, min: 100, max: 900, defaultVal: 400, step: 1 },
        { axis: 'wdth', label: t.fontWidth, min: 75, max: 125, defaultVal: 100, step: 1 },
        { axis: 'slnt', label: t.fontSlant, min: -12, max: 0, defaultVal: 0, step: 1 },
        { axis: 'ROND', label: t.fontRound, min: 0, max: 100, defaultVal: 0, step: 1 },
      ] as const).map(({ axis, label, min, max, defaultVal, step }) => {
        const value = (resolvedStyle.fontVariations as Record<string, number | undefined> | undefined)?.[axis] ?? defaultVal;
        return (
          <SettingRow
            key={axis}
            label={label}
            valueDisplay={`${value}`}
            className={`subtitle-font-axis-${axis.toLowerCase()}-field`}
          >
            <Slider
              min={min}
              max={max}
              step={step}
              value={value}
              onPointerDown={beginBatch}
              onPointerUp={commitBatch}
              onChange={(nextValue) => onUpdateSelectedSubtitles((subtitle) => ({
                ...subtitle,
                style: {
                  ...subtitle.style,
                  fontVariations: {
                    ...(subtitle.style.fontVariations ?? {}),
                    [axis]: nextValue,
                  },
                },
              }))}
            />
          </SettingRow>
        );
      })}

      <div className="subtitle-align-row flex items-center gap-3">
        <span className="w-20 flex-shrink-0 text-[11px] font-medium text-on-surface-variant">
          {t.textAlignment}
        </span>
        <div className="subtitle-align-button-group ui-segmented overflow-hidden">
          {(['left', 'center', 'right'] as const).map((align) => {
            const Icon = align === 'left' ? AlignLeft : align === 'center' ? AlignCenter : AlignRight;
            const isActive = (resolvedStyle.textAlign ?? 'center') === align;
            return (
              <button
                key={align}
                type="button"
                onClick={() => onUpdateSelectedSubtitles((subtitle) => ({
                  ...subtitle,
                  style: { ...subtitle.style, textAlign: align },
                }))}
                className={`subtitle-align-button ui-segmented-button flex h-7 w-7 items-center justify-center ${
                  isActive ? 'ui-segmented-button-active' : ''
                }`}
                title={align}
              >
                <Icon className="h-3.5 w-3.5" />
              </button>
            );
          })}
        </div>
      </div>

      <SettingRow label={t.opacity} valueDisplay={`${Math.round((resolvedStyle.opacity ?? 1) * 100)}%`}>
        <Slider
          min={0}
          max={1}
          step={0.01}
          value={resolvedStyle.opacity ?? 1}
          onPointerDown={beginBatch}
          onPointerUp={commitBatch}
          onChange={(value) => onUpdateSelectedSubtitles((subtitle) => ({
            ...subtitle,
            style: { ...subtitle.style, opacity: value },
          }))}
        />
      </SettingRow>

      <SettingRow label={t.letterSpacing} valueDisplay={`${resolvedStyle.letterSpacing ?? 0}`}>
        <Slider
          min={-5}
          max={20}
          step={1}
          value={resolvedStyle.letterSpacing ?? 0}
          onPointerDown={beginBatch}
          onPointerUp={commitBatch}
          onChange={(value) => onUpdateSelectedSubtitles((subtitle) => ({
            ...subtitle,
            style: { ...subtitle.style, letterSpacing: value },
          }))}
        />
      </SettingRow>

      <SettingRow label={t.lineHeight} valueDisplay={`${(resolvedStyle.lineHeight ?? 1.25).toFixed(2)}x`}>
        <Slider
          min={0.8}
          max={2}
          step={0.01}
          value={resolvedStyle.lineHeight ?? 1.25}
          onPointerDown={beginBatch}
          onPointerUp={commitBatch}
          onChange={(value) => onUpdateSelectedSubtitles((subtitle) => ({
            ...subtitle,
            style: { ...subtitle.style, lineHeight: value },
          }))}
        />
      </SettingRow>

      <div>
        <label className="subtitle-wrap-toggle flex items-center gap-3 text-[11px] text-on-surface-variant cursor-pointer">
          <Checkbox
            checked={resolvedStyle.wrap?.enabled ?? true}
            onChange={(event) => onUpdateSelectedSubtitles((subtitle) => ({
              ...subtitle,
              style: {
                ...subtitle.style,
                wrap: {
                  ...(resolvedStyle.wrap ?? { enabled: true, maxWidthPercent: 80 }),
                  enabled: event.target.checked,
                },
              },
            }))}
          />
          {t.wrapText}
        </label>
        {resolvedStyle.wrap?.enabled ? (
          <div className="subtitle-wrap-controls mt-1 space-y-3.5 pl-1">
            <SettingRow label={t.maxWidth} valueDisplay={`${resolvedStyle.wrap.maxWidthPercent}%`}>
              <Slider
                min={20}
                max={100}
                step={1}
                value={resolvedStyle.wrap.maxWidthPercent}
                onPointerDown={beginBatch}
                onPointerUp={commitBatch}
                onChange={(value) => onUpdateSelectedSubtitles((subtitle) => ({
                  ...subtitle,
                  style: {
                    ...subtitle.style,
                    wrap: { ...(resolvedStyle.wrap ?? { enabled: true, maxWidthPercent: 80 }), maxWidthPercent: value },
                  },
                }))}
              />
            </SettingRow>
          </div>
        ) : null}
      </div>

      <SubtitleEffectControls
        t={t}
        resolvedStyle={resolvedStyle}
        onUpdateSelectedSubtitles={onUpdateSelectedSubtitles}
        beginBatch={beginBatch}
        commitBatch={commitBatch}
      />
    </div>
  );
}
