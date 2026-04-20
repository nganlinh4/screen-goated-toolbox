import { AlignLeft, AlignCenter, AlignRight } from 'lucide-react';
import { VideoSegment, TextSegment } from '@/types/video';
import { ColorPicker } from '@/components/ui/ColorPicker';
import { Checkbox } from '@/components/ui/checkbox';
import { Slider } from '@/components/ui/Slider';
import { PanelSelect } from '@/components/ui/PanelSelect';
import { PanelCard } from '@/components/layout/PanelCard';
import { SettingRow } from '@/components/layout/SettingRow';
import { useSettings } from '@/hooks/useSettings';
import { normalizeTextStyle } from '@/lib/textStyleDefaults';

function buildFontVariationCSS(vars?: TextSegment['style']['fontVariations']): string | undefined {
  const parts: string[] = [];
  if (vars?.wdth !== undefined && vars.wdth !== 100) parts.push(`'wdth' ${vars.wdth}`);
  if (vars?.slnt !== undefined && vars.slnt !== 0) parts.push(`'slnt' ${vars.slnt}`);
  if (vars?.ROND !== undefined && vars.ROND !== 0) parts.push(`'ROND' ${vars.ROND}`);
  return parts.length > 0 ? parts.join(', ') : undefined;
}

export interface TextPanelProps {
  segment: VideoSegment | null;
  editingTextId: string | null;
  selectedTextIds?: string[];
  onUpdateSegment: (segment: VideoSegment) => void;
  beginBatch: () => void;
  commitBatch: () => void;
}

export function TextPanel({ segment, editingTextId, selectedTextIds, onUpdateSegment, beginBatch, commitBatch }: TextPanelProps) {
  const { t } = useSettings();

  // Selection mode: selectedTextIds has 1+ entries → use first as source
  const hasSelection = selectedTextIds && selectedTextIds.length >= 1;
  const isMultiSelect = selectedTextIds && selectedTextIds.length >= 2;
  const selectionSet = hasSelection ? new Set(selectedTextIds) : null;
  const sourceId = hasSelection ? selectedTextIds![0] : editingTextId;
  const editingText = sourceId ? segment?.textSegments?.find(ts => ts.id === sourceId) : null;
  const resolvedStyle = editingText ? normalizeTextStyle(editingText.style) : null;

  const updateStyle = (updates: Partial<TextSegment['style']>) => {
    if (!segment || !sourceId) return;
    if (hasSelection && selectionSet) {
      // Apply style changes to ALL selected segments
      onUpdateSegment({
        ...segment,
        textSegments: segment.textSegments.map(ts =>
          selectionSet.has(ts.id) ? { ...ts, style: { ...ts.style, ...updates } } : ts
        )
      });
    } else {
      onUpdateSegment({
        ...segment,
        textSegments: segment.textSegments.map(ts =>
          ts.id === sourceId ? { ...ts, style: { ...ts.style, ...updates } } : ts
        )
      });
    }
  };

  const updateText = (text: string) => {
    if (!segment || !sourceId) return;
    if (hasSelection && selectionSet) {
      // Apply text to all selected segments
      onUpdateSegment({
        ...segment,
        textSegments: segment.textSegments.map(ts =>
          selectionSet.has(ts.id) ? { ...ts, text } : ts
        )
      });
    } else {
      onUpdateSegment({
        ...segment,
        textSegments: segment.textSegments.map(ts =>
          ts.id === sourceId ? { ...ts, text } : ts
        )
      });
    }
  };

  return (
    <PanelCard className="text-panel">
      {editingText && segment && resolvedStyle ? (
        <div className="text-controls space-y-3.5">
          {isMultiSelect && (
            <div className="text-multi-select-badge text-[10px] font-medium px-2 py-1 rounded-md" style={{ background: 'color-mix(in srgb, var(--timeline-zoom-color) 15%, transparent)', color: 'var(--timeline-zoom-color)' }}>
              {selectedTextIds!.length} {t.textMultiSelectLabel}
            </div>
          )}

          <textarea
            value={editingText.text}
            onFocus={beginBatch}
            onBlur={commitBatch}
            onChange={(e) => updateText(e.target.value)}
            className="text-editor-input ui-input w-full rounded-xl px-3 py-2 text-on-surface text-sm thin-scrollbar subtle-resize"
            style={{
              fontFamily: "'Google Sans Flex', 'Segoe UI', system-ui, sans-serif",
              fontWeight: resolvedStyle.fontVariations?.wght ?? 400,
              fontVariationSettings: buildFontVariationCSS(resolvedStyle.fontVariations),
            }}
            rows={2}
          />

          <p className="text-[10px] text-on-surface-variant">{t.dragTextHint}</p>

          <SettingRow label={t.fontSize} valueDisplay={`${resolvedStyle.fontSize}`}>
            <Slider
              min={12} max={200} step={1} value={resolvedStyle.fontSize}
              onPointerDown={beginBatch} onPointerUp={commitBatch}
              onChange={(val) => updateStyle({ fontSize: val })}
            />
          </SettingRow>

          <div className="color-field flex items-center gap-3">
            <span className="text-[11px] font-medium text-on-surface-variant w-20 flex-shrink-0">{t.color}</span>
            <ColorPicker
              value={resolvedStyle.color}
              onChange={(color) => updateStyle({ color })}
              onOpen={beginBatch}
              onClose={commitBatch}
            />
          </div>

          {/* Font Variation Axes */}
          {([
            { axis: 'wght', label: t.fontWeight, min: 100, max: 900, defaultVal: 400, step: 1 },
            { axis: 'wdth', label: t.fontWidth, min: 75, max: 125, defaultVal: 100, step: 1 },
            { axis: 'slnt', label: t.fontSlant, min: -12, max: 0, defaultVal: 0, step: 1 },
            { axis: 'ROND', label: t.fontRound, min: 0, max: 100, defaultVal: 0, step: 1 },
          ] as const).map(({ axis, label, min, max, defaultVal, step }) => {
            const value = (resolvedStyle.fontVariations as any)?.[axis] ?? defaultVal;
            return (
              <SettingRow key={axis} label={label} valueDisplay={`${value}`} className={`font-axis-${axis.toLowerCase()}-field`}>
                <Slider
                  min={min} max={max} step={step} value={value}
                  onPointerDown={beginBatch} onPointerUp={commitBatch}
                  onChange={(val) => updateStyle({
                    fontVariations: { ...(resolvedStyle.fontVariations || {}), [axis]: val }
                  })}
                />
              </SettingRow>
            );
          })}

          <div className="text-align-field flex items-center gap-3">
            <span className="text-[11px] font-medium text-on-surface-variant w-20 flex-shrink-0">{t.textAlignment}</span>
            <div className="alignment-button-group ui-segmented overflow-hidden">
              {(['left', 'center', 'right'] as const).map(align => {
                const Icon = align === 'left' ? AlignLeft : align === 'center' ? AlignCenter : AlignRight;
                const isActive = (resolvedStyle.textAlign ?? 'center') === align;
                return (
                  <button
                    key={align}
                    onClick={() => updateStyle({ textAlign: align })}
                    className={`text-align-button ui-segmented-button flex items-center justify-center w-7 h-7 ${
                      isActive
                        ? 'ui-segmented-button-active'
                        : ''
                    }`}
                    title={align}
                  >
                    <Icon className="w-3.5 h-3.5" />
                  </button>
                );
              })}
            </div>
          </div>

          <SettingRow label={t.opacity} valueDisplay={`${Math.round((resolvedStyle.opacity ?? 1) * 100)}%`}>
            <Slider
              min={0} max={1} step={0.01} value={resolvedStyle.opacity ?? 1}
              onPointerDown={beginBatch} onPointerUp={commitBatch}
              onChange={(val) => updateStyle({ opacity: val })}
            />
          </SettingRow>

          <SettingRow label={t.letterSpacing} valueDisplay={`${resolvedStyle.letterSpacing ?? 0}`}>
            <Slider
              min={-5} max={20} step={1} value={resolvedStyle.letterSpacing ?? 0}
              onPointerDown={beginBatch} onPointerUp={commitBatch}
              onChange={(val) => updateStyle({ letterSpacing: val })}
            />
          </SettingRow>

          <SettingRow label={t.lineHeight} valueDisplay={`${(resolvedStyle.lineHeight ?? 1.25).toFixed(2)}x`}>
            <Slider
              min={0.8} max={2} step={0.01} value={resolvedStyle.lineHeight ?? 1.25}
              onPointerDown={beginBatch} onPointerUp={commitBatch}
              onChange={(val) => updateStyle({ lineHeight: val })}
            />
          </SettingRow>

          <div>
            <label className="flex items-center gap-3 text-[10px] text-on-surface-variant cursor-pointer">
              <Checkbox
                checked={resolvedStyle.wrap?.enabled ?? true}
                onChange={(e) => updateStyle({
                  wrap: {
                    ...(resolvedStyle.wrap ?? { enabled: true, maxWidthPercent: 80 }),
                    enabled: e.target.checked,
                  }
                })}
              />
              {t.wrapText}
            </label>
            {resolvedStyle.wrap?.enabled && (
              <div className="text-wrap-controls mt-1 space-y-3.5 pl-1">
                <SettingRow label={t.maxWidth} valueDisplay={`${resolvedStyle.wrap.maxWidthPercent}%`}>
                  <Slider
                    min={20} max={100} step={1} value={resolvedStyle.wrap.maxWidthPercent}
                    onPointerDown={beginBatch} onPointerUp={commitBatch}
                    onChange={(val) => updateStyle({
                      wrap: { ...resolvedStyle.wrap!, maxWidthPercent: val }
                    })}
                  />
                </SettingRow>
              </div>
            )}
          </div>

          <div>
            <div className="text-animation-row flex items-center gap-3">
              <span className="text-[11px] font-medium text-on-surface-variant w-20 flex-shrink-0">{t.animation}</span>
              <PanelSelect
                value={resolvedStyle.animation?.preset ?? 'fade'}
                options={[
                  { value: 'none', label: t.animationPresetNone },
                  { value: 'fade', label: t.animationPresetFade },
                  { value: 'slide-up', label: t.animationPresetSlideUp },
                  { value: 'pop', label: t.animationPresetPop },
                ]}
                onChange={(value) => updateStyle({
                  animation: { ...(resolvedStyle.animation ?? { preset: 'fade', inDuration: 0.3, outDuration: 0.3 }), preset: value as any }
                })}
                triggerClassName="text-animation-select h-8 flex-1 rounded-lg px-2.5 text-[11px]"
                contentClassName="text-animation-menu"
              />
            </div>
            {(resolvedStyle.animation?.preset ?? 'fade') !== 'none' && (
              <div className="text-animation-controls mt-1 space-y-3.5 pl-1">
                <SettingRow label={t.animationInDuration} valueDisplay={`${(resolvedStyle.animation?.inDuration ?? 0.3).toFixed(2)}s`}>
                  <Slider
                    min={0.05} max={1.5} step={0.01} value={resolvedStyle.animation?.inDuration ?? 0.3}
                    onPointerDown={beginBatch} onPointerUp={commitBatch}
                    onChange={(val) => updateStyle({
                      animation: { ...(resolvedStyle.animation ?? { preset: 'fade', inDuration: 0.3, outDuration: 0.3 }), inDuration: val }
                    })}
                  />
                </SettingRow>
                <SettingRow label={t.animationOutDuration} valueDisplay={`${(resolvedStyle.animation?.outDuration ?? 0.3).toFixed(2)}s`}>
                  <Slider
                    min={0.05} max={1.5} step={0.01} value={resolvedStyle.animation?.outDuration ?? 0.3}
                    onPointerDown={beginBatch} onPointerUp={commitBatch}
                    onChange={(val) => updateStyle({
                      animation: { ...(resolvedStyle.animation ?? { preset: 'fade', inDuration: 0.3, outDuration: 0.3 }), outDuration: val }
                    })}
                  />
                </SettingRow>
              </div>
            )}
          </div>

          {/* Background Pill */}
          <div>
            <label className="flex items-center gap-3 text-[10px] text-on-surface-variant cursor-pointer">
              <Checkbox
                checked={resolvedStyle.background?.enabled ?? false}
                onChange={(e) => updateStyle({
                  background: {
                    enabled: e.target.checked,
                    color: resolvedStyle.background?.color ?? '#000000',
                    opacity: resolvedStyle.background?.opacity ?? 0.6,
                    paddingX: resolvedStyle.background?.paddingX ?? 16,
                    paddingY: resolvedStyle.background?.paddingY ?? 8,
                    borderRadius: resolvedStyle.background?.borderRadius ?? 32
                  }
                })}
              />
              {t.backgroundPill}
            </label>
            {resolvedStyle.background?.enabled && (
              <div className="background-pill-controls space-y-3.5 mt-1 pl-1">
                <div className="pill-color-field flex items-center gap-3">
                  <span className="text-[11px] font-medium text-on-surface-variant w-20 flex-shrink-0">{t.pillColor}</span>
                  <ColorPicker
                    value={resolvedStyle.background.color.startsWith('rgba') ? '#000000' : resolvedStyle.background.color}
                    onChange={(color) => updateStyle({
                      background: { ...resolvedStyle.background!, color }
                    })}
                    onOpen={beginBatch}
                    onClose={commitBatch}
                  />
                </div>
                <SettingRow label={t.pillOpacity} valueDisplay={`${Math.round((resolvedStyle.background.opacity ?? 0.6) * 100)}%`} className="pill-opacity-field">
                  <Slider
                    min={0} max={1} step={0.01} value={resolvedStyle.background.opacity ?? 0.6}
                    onPointerDown={beginBatch} onPointerUp={commitBatch}
                    onChange={(val) => updateStyle({
                      background: { ...resolvedStyle.background!, opacity: val }
                    })}
                  />
                </SettingRow>
                <SettingRow label={t.paddingX} valueDisplay={`${resolvedStyle.background.paddingX}`} className="pill-padding-x-field">
                  <Slider
                    min={0} max={64} step={1} value={resolvedStyle.background.paddingX}
                    onPointerDown={beginBatch} onPointerUp={commitBatch}
                    onChange={(val) => updateStyle({
                      background: { ...resolvedStyle.background!, paddingX: val }
                    })}
                  />
                </SettingRow>
                <SettingRow label={t.paddingY} valueDisplay={`${resolvedStyle.background.paddingY}`} className="pill-padding-y-field">
                  <Slider
                    min={0} max={48} step={1} value={resolvedStyle.background.paddingY}
                    onPointerDown={beginBatch} onPointerUp={commitBatch}
                    onChange={(val) => updateStyle({
                      background: { ...resolvedStyle.background!, paddingY: val }
                    })}
                  />
                </SettingRow>
                <SettingRow label={t.pillRadius} valueDisplay={`${resolvedStyle.background.borderRadius}`} className="pill-radius-field">
                  <Slider
                    min={0} max={32} step={1} value={resolvedStyle.background.borderRadius}
                    onPointerDown={beginBatch} onPointerUp={commitBatch}
                    onChange={(val) => updateStyle({
                      background: { ...resolvedStyle.background!, borderRadius: val }
                    })}
                  />
                </SettingRow>
              </div>
            )}
          </div>

          <div>
            <label className="flex items-center gap-3 text-[10px] text-on-surface-variant cursor-pointer">
              <Checkbox
                checked={resolvedStyle.stroke?.enabled ?? false}
                onChange={(e) => updateStyle({
                  stroke: { ...(resolvedStyle.stroke ?? { enabled: false, color: '#000000', width: 2, opacity: 1 }), enabled: e.target.checked }
                })}
              />
              {t.stroke}
            </label>
            {resolvedStyle.stroke?.enabled && (
              <div className="text-stroke-controls mt-1 space-y-3.5 pl-1">
                <div className="stroke-color-field flex items-center gap-3">
                  <span className="text-[11px] font-medium text-on-surface-variant w-20 flex-shrink-0">{t.strokeColor}</span>
                  <ColorPicker
                    value={resolvedStyle.stroke.color}
                    onChange={(color) => updateStyle({ stroke: { ...resolvedStyle.stroke!, color } })}
                    onOpen={beginBatch}
                    onClose={commitBatch}
                  />
                </div>
                <SettingRow label={t.strokeWidth} valueDisplay={`${resolvedStyle.stroke.width}`}>
                  <Slider
                    min={0} max={16} step={0.5} value={resolvedStyle.stroke.width}
                    onPointerDown={beginBatch} onPointerUp={commitBatch}
                    onChange={(val) => updateStyle({ stroke: { ...resolvedStyle.stroke!, width: val } })}
                  />
                </SettingRow>
                <SettingRow label={t.strokeOpacity} valueDisplay={`${Math.round((resolvedStyle.stroke.opacity ?? 1) * 100)}%`}>
                  <Slider
                    min={0} max={1} step={0.01} value={resolvedStyle.stroke.opacity ?? 1}
                    onPointerDown={beginBatch} onPointerUp={commitBatch}
                    onChange={(val) => updateStyle({ stroke: { ...resolvedStyle.stroke!, opacity: val } })}
                  />
                </SettingRow>
              </div>
            )}
          </div>

          <div>
            <label className="flex items-center gap-3 text-[10px] text-on-surface-variant cursor-pointer">
              <Checkbox
                checked={resolvedStyle.shadow?.enabled ?? true}
                onChange={(e) => updateStyle({
                  shadow: { ...(resolvedStyle.shadow ?? { enabled: true, color: '#000000', blur: 4, offsetX: 2, offsetY: 2, opacity: 0.7 }), enabled: e.target.checked }
                })}
              />
              {t.shadow}
            </label>
            {resolvedStyle.shadow?.enabled && (
              <div className="text-shadow-controls mt-1 space-y-3.5 pl-1">
                <div className="shadow-color-field flex items-center gap-3">
                  <span className="text-[11px] font-medium text-on-surface-variant w-20 flex-shrink-0">{t.shadowColor}</span>
                  <ColorPicker
                    value={resolvedStyle.shadow.color}
                    onChange={(color) => updateStyle({ shadow: { ...resolvedStyle.shadow!, color } })}
                    onOpen={beginBatch}
                    onClose={commitBatch}
                  />
                </div>
                <SettingRow label={t.shadowBlur} valueDisplay={`${resolvedStyle.shadow.blur}`}>
                  <Slider
                    min={0} max={32} step={1} value={resolvedStyle.shadow.blur}
                    onPointerDown={beginBatch} onPointerUp={commitBatch}
                    onChange={(val) => updateStyle({ shadow: { ...resolvedStyle.shadow!, blur: val } })}
                  />
                </SettingRow>
                <SettingRow label={t.shadowOffsetX} valueDisplay={`${resolvedStyle.shadow.offsetX}`}>
                  <Slider
                    min={-24} max={24} step={1} value={resolvedStyle.shadow.offsetX}
                    onPointerDown={beginBatch} onPointerUp={commitBatch}
                    onChange={(val) => updateStyle({ shadow: { ...resolvedStyle.shadow!, offsetX: val } })}
                  />
                </SettingRow>
                <SettingRow label={t.shadowOffsetY} valueDisplay={`${resolvedStyle.shadow.offsetY}`}>
                  <Slider
                    min={-24} max={24} step={1} value={resolvedStyle.shadow.offsetY}
                    onPointerDown={beginBatch} onPointerUp={commitBatch}
                    onChange={(val) => updateStyle({ shadow: { ...resolvedStyle.shadow!, offsetY: val } })}
                  />
                </SettingRow>
                <SettingRow label={t.shadowOpacity} valueDisplay={`${Math.round((resolvedStyle.shadow.opacity ?? 0.7) * 100)}%`}>
                  <Slider
                    min={0} max={1} step={0.01} value={resolvedStyle.shadow.opacity ?? 0.7}
                    onPointerDown={beginBatch} onPointerUp={commitBatch}
                    onChange={(val) => updateStyle({ shadow: { ...resolvedStyle.shadow!, opacity: val } })}
                  />
                </SettingRow>
              </div>
            )}
          </div>
        </div>
      ) : (
        <p className="text-xs text-on-surface-variant">{t.textPanelHint}</p>
      )}
    </PanelCard>
  );
}
