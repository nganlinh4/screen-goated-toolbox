import { AlignLeft, AlignCenter, AlignRight } from 'lucide-react';
import { VideoSegment, TextSegment } from '@/types/video';
import { ColorPicker } from '@/components/ui/ColorPicker';
import { Slider } from '@/components/ui/Slider';
import { PanelCard } from '@/components/layout/PanelCard';
import { SettingRow } from '@/components/layout/SettingRow';
import { useSettings } from '@/hooks/useSettings';

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
  onUpdateSegment: (segment: VideoSegment) => void;
  beginBatch: () => void;
  commitBatch: () => void;
}

export function TextPanel({ segment, editingTextId, onUpdateSegment, beginBatch, commitBatch }: TextPanelProps) {
  const { t } = useSettings();
  const editingText = editingTextId ? segment?.textSegments?.find(ts => ts.id === editingTextId) : null;

  const updateStyle = (updates: Partial<TextSegment['style']>) => {
    if (!segment || !editingTextId) return;
    onUpdateSegment({
      ...segment,
      textSegments: segment.textSegments.map(ts =>
        ts.id === editingTextId ? { ...ts, style: { ...ts.style, ...updates } } : ts
      )
    });
  };

  return (
    <PanelCard className="text-panel">
      {editingText && segment ? (
        <div className="text-controls space-y-3.5">
          <textarea
            value={editingText.text}
            onFocus={beginBatch}
            onBlur={commitBatch}
            onChange={(e) => {
              onUpdateSegment({
                ...segment,
                textSegments: segment.textSegments.map(ts =>
                  ts.id === editingTextId ? { ...ts, text: e.target.value } : ts
                )
              });
            }}
            className="w-full bg-glass-bg border border-glass-border rounded-lg px-3 py-2 text-on-surface text-sm focus:border-[var(--primary-color)]/50 focus:ring-1 focus:ring-[var(--primary-color)]/30 transition-colors thin-scrollbar subtle-resize"
            style={{
              fontFamily: "'Google Sans Flex', 'Segoe UI', system-ui, sans-serif",
              fontWeight: editingText.style.fontVariations?.wght ?? 400,
              fontVariationSettings: buildFontVariationCSS(editingText.style.fontVariations),
            }}
            rows={2}
          />

          <p className="text-[10px] text-on-surface-variant">{t.dragTextHint}</p>

          <SettingRow label={t.fontSize} valueDisplay={`${editingText.style.fontSize}`}>
            <Slider
              min={12} max={200} step={1} value={editingText.style.fontSize}
              onPointerDown={beginBatch} onPointerUp={commitBatch}
              onChange={(val) => updateStyle({ fontSize: val })}
            />
          </SettingRow>

          <div className="color-field flex items-center gap-3">
            <span className="text-[11px] font-medium text-on-surface-variant w-20 flex-shrink-0">{t.color}</span>
            <ColorPicker
              value={editingText.style.color}
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
            const value = (editingText.style.fontVariations as any)?.[axis] ?? defaultVal;
            return (
              <SettingRow key={axis} label={label} valueDisplay={`${value}`} className={`font-axis-${axis.toLowerCase()}-field`}>
                <Slider
                  min={min} max={max} step={step} value={value}
                  onPointerDown={beginBatch} onPointerUp={commitBatch}
                  onChange={(val) => updateStyle({
                    fontVariations: { ...(editingText.style.fontVariations || {}), [axis]: val }
                  })}
                />
              </SettingRow>
            );
          })}

          <div className="text-align-field flex items-center gap-3">
            <span className="text-[11px] font-medium text-on-surface-variant w-20 flex-shrink-0">{t.textAlignment}</span>
            <div className="alignment-button-group flex rounded-lg border border-glass-border overflow-hidden">
              {(['left', 'center', 'right'] as const).map(align => {
                const Icon = align === 'left' ? AlignLeft : align === 'center' ? AlignCenter : AlignRight;
                const isActive = (editingText.style.textAlign ?? 'center') === align;
                return (
                  <button
                    key={align}
                    onClick={() => updateStyle({ textAlign: align })}
                    className={`flex items-center justify-center w-7 h-7 transition-colors ${
                      isActive
                        ? 'bg-[var(--primary-color)]/20 text-[var(--primary-color)]'
                        : 'bg-glass-bg text-on-surface-variant hover:text-on-surface'
                    }`}
                    title={align}
                  >
                    <Icon className="w-3.5 h-3.5" />
                  </button>
                );
              })}
            </div>
          </div>

          <SettingRow label={t.opacity} valueDisplay={`${Math.round((editingText.style.opacity ?? 1) * 100)}%`}>
            <Slider
              min={0} max={1} step={0.01} value={editingText.style.opacity ?? 1}
              onPointerDown={beginBatch} onPointerUp={commitBatch}
              onChange={(val) => updateStyle({ opacity: val })}
            />
          </SettingRow>

          <SettingRow label={t.letterSpacing} valueDisplay={`${editingText.style.letterSpacing ?? 0}`}>
            <Slider
              min={-5} max={20} step={1} value={editingText.style.letterSpacing ?? 0}
              onPointerDown={beginBatch} onPointerUp={commitBatch}
              onChange={(val) => updateStyle({ letterSpacing: val })}
            />
          </SettingRow>

          {/* Background Pill */}
          <div>
            <label className="flex items-center gap-3 text-[10px] text-on-surface-variant cursor-pointer">
              <input
                type="checkbox"
                checked={editingText.style.background?.enabled ?? false}
                onChange={(e) => updateStyle({
                  background: {
                    enabled: e.target.checked,
                    color: editingText.style.background?.color ?? '#000000',
                    opacity: editingText.style.background?.opacity ?? 0.6,
                    paddingX: editingText.style.background?.paddingX ?? 16,
                    paddingY: editingText.style.background?.paddingY ?? 8,
                    borderRadius: editingText.style.background?.borderRadius ?? 32
                  }
                })}
                className="rounded"
              />
              {t.backgroundPill}
            </label>
            {editingText.style.background?.enabled && (
              <div className="background-pill-controls space-y-3.5 mt-1 pl-1">
                <div className="pill-color-field flex items-center gap-3">
                  <span className="text-[11px] font-medium text-on-surface-variant w-20 flex-shrink-0">{t.pillColor}</span>
                  <ColorPicker
                    value={editingText.style.background.color.startsWith('rgba') ? '#000000' : editingText.style.background.color}
                    onChange={(color) => updateStyle({
                      background: { ...editingText.style.background!, color }
                    })}
                    onOpen={beginBatch}
                    onClose={commitBatch}
                  />
                </div>
                <SettingRow label={t.pillOpacity} valueDisplay={`${Math.round((editingText.style.background.opacity ?? 0.6) * 100)}%`} className="pill-opacity-field">
                  <Slider
                    min={0} max={1} step={0.01} value={editingText.style.background.opacity ?? 0.6}
                    onPointerDown={beginBatch} onPointerUp={commitBatch}
                    onChange={(val) => updateStyle({
                      background: { ...editingText.style.background!, opacity: val }
                    })}
                  />
                </SettingRow>
                <SettingRow label={t.pillRadius} valueDisplay={`${editingText.style.background.borderRadius}`} className="pill-radius-field">
                  <Slider
                    min={0} max={32} step={1} value={editingText.style.background.borderRadius}
                    onPointerDown={beginBatch} onPointerUp={commitBatch}
                    onChange={(val) => updateStyle({
                      background: { ...editingText.style.background!, borderRadius: val }
                    })}
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
