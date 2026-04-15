import { AlignCenter } from 'lucide-react';
import { PanelCard } from '@/components/layout/PanelCard';
import { SettingRow } from '@/components/layout/SettingRow';
import { ColorPicker } from '@/components/ui/ColorPicker';
import { Slider } from '@/components/ui/Slider';
import { Checkbox } from '@/components/ui/checkbox';
import { useSettings } from '@/hooks/useSettings';
import { VideoSegment } from '@/types/video';

export interface SubtitlePanelProps {
  segment: VideoSegment | null;
  selectedSubtitleIds?: string[];
  selectedSource: 'video' | 'mic';
  onSourceChange: (value: 'video' | 'mic') => void;
  languageHint: string;
  onLanguageHintChange: (value: string) => void;
  isGenerating: boolean;
  statusMessage?: string | null;
  canUseVideoSource: boolean;
  canUseMicSource: boolean;
  onGenerate: () => void;
  onCancel: () => void;
  onUpdateSegment: (segment: VideoSegment) => void;
  beginBatch: () => void;
  commitBatch: () => void;
}

export function SubtitlePanel({
  segment,
  selectedSubtitleIds,
  selectedSource,
  onSourceChange,
  languageHint,
  onLanguageHintChange,
  isGenerating,
  statusMessage,
  canUseVideoSource,
  canUseMicSource,
  onGenerate,
  onCancel,
  onUpdateSegment,
  beginBatch,
  commitBatch,
}: SubtitlePanelProps) {
  const { t } = useSettings();
  const selection = (selectedSubtitleIds?.length ?? 0) > 0
    ? new Set(selectedSubtitleIds)
    : null;
  const sourceSubtitle = selection
    ? (segment?.subtitleSegments ?? []).find((subtitle) => selection.has(subtitle.id)) ?? null
    : (segment?.subtitleSegments ?? [])[0] ?? null;
  const editableSubtitles = selection
    ? (segment?.subtitleSegments ?? []).filter((subtitle) => selection.has(subtitle.id))
    : (segment?.subtitleSegments ?? []);
  const hasSubtitleSource = canUseVideoSource || canUseMicSource;
  const hasSubtitles = (segment?.subtitleSegments?.length ?? 0) > 0;

  const updateSelectedSubtitles = (updater: (subtitle: NonNullable<typeof sourceSubtitle>) => NonNullable<typeof sourceSubtitle>) => {
    if (!segment || !sourceSubtitle) return;
    const targetIds = selection ? selection : new Set([sourceSubtitle.id]);
    onUpdateSegment({
      ...segment,
      subtitleSegments: (segment.subtitleSegments ?? []).map((subtitle) =>
        targetIds.has(subtitle.id) ? updater(subtitle) : subtitle,
      ),
    });
  };

  return (
    <PanelCard className="subtitle-panel">
      <div className="space-y-3.5">
        <p className="text-xs text-on-surface-variant">{t.subtitlePanelHint}</p>

        <div className="subtitle-source-row flex items-center gap-2">
          <span className="w-20 flex-shrink-0 text-[11px] font-medium text-on-surface-variant">
            {t.subtitleSource}
          </span>
          <select
            value={selectedSource}
            onChange={(e) => onSourceChange(e.target.value as 'video' | 'mic')}
            className="ui-input h-9 flex-1 rounded-xl px-3 text-sm"
          >
            <option value="video" disabled={!canUseVideoSource}>{t.subtitleSourceVideo}</option>
            <option value="mic" disabled={!canUseMicSource}>{t.subtitleSourceMic}</option>
          </select>
        </div>

        <div className="subtitle-language-row flex items-center gap-2">
          <span className="w-20 flex-shrink-0 text-[11px] font-medium text-on-surface-variant">
            {t.subtitleLanguageHint}
          </span>
          <input
            value={languageHint}
            onChange={(e) => onLanguageHintChange(e.target.value)}
            placeholder="auto"
            className="ui-input h-9 flex-1 rounded-xl px-3 text-sm"
          />
        </div>

        <div className="subtitle-actions flex gap-2">
          <button
            type="button"
            disabled={isGenerating || !hasSubtitleSource}
            onClick={onGenerate}
            className="ui-button flex-1 rounded-xl px-3 py-2 text-sm font-medium disabled:opacity-50"
          >
            {hasSubtitles ? t.subtitleRegenerate : t.subtitleGenerate}
          </button>
          <button
            type="button"
            disabled={!isGenerating}
            onClick={onCancel}
            className="ui-button flex-1 rounded-xl px-3 py-2 text-sm font-medium disabled:opacity-50"
          >
            {t.subtitleCancelJob}
          </button>
        </div>

        <p className="text-[11px] text-on-surface-variant">
          {statusMessage ?? (hasSubtitleSource ? t.subtitleIdleHint : t.subtitleUnavailableSource)}
        </p>

        {sourceSubtitle && editableSubtitles.length > 0 ? (
          <div className="subtitle-style-controls space-y-3.5">
            <div className="subtitle-preview-badge inline-flex items-center gap-2 rounded-full px-3 py-1 text-[10px] font-medium"
              style={{ background: 'color-mix(in srgb, var(--timeline-zoom-color) 15%, transparent)', color: 'var(--timeline-zoom-color)' }}>
              <AlignCenter className="w-3 h-3" />
              {selection ? `${editableSubtitles.length} ${t.trackSubtitles}` : t.trackSubtitles}
            </div>

            <SettingRow label={t.fontSize} valueDisplay={`${sourceSubtitle.style.fontSize}`}>
              <Slider
                min={12}
                max={200}
                step={1}
                value={sourceSubtitle.style.fontSize}
                onPointerDown={beginBatch}
                onPointerUp={commitBatch}
                onChange={(value) => updateSelectedSubtitles((subtitle) => ({
                  ...subtitle,
                  style: { ...subtitle.style, fontSize: value },
                }))}
              />
            </SettingRow>

            <div className="subtitle-color-row flex items-center gap-3">
              <span className="w-20 flex-shrink-0 text-[11px] font-medium text-on-surface-variant">{t.color}</span>
              <ColorPicker
                value={sourceSubtitle.style.color}
                onChange={(color) => updateSelectedSubtitles((subtitle) => ({
                  ...subtitle,
                  style: { ...subtitle.style, color },
                }))}
                onOpen={beginBatch}
                onClose={commitBatch}
              />
            </div>

            <SettingRow label={t.opacity} valueDisplay={`${Math.round((sourceSubtitle.style.opacity ?? 1) * 100)}%`}>
              <Slider
                min={0}
                max={1}
                step={0.01}
                value={sourceSubtitle.style.opacity ?? 1}
                onPointerDown={beginBatch}
                onPointerUp={commitBatch}
                onChange={(value) => updateSelectedSubtitles((subtitle) => ({
                  ...subtitle,
                  style: { ...subtitle.style, opacity: value },
                }))}
              />
            </SettingRow>

            <SettingRow label={t.letterSpacing} valueDisplay={`${sourceSubtitle.style.letterSpacing ?? 0}`}>
              <Slider
                min={-5}
                max={20}
                step={1}
                value={sourceSubtitle.style.letterSpacing ?? 0}
                onPointerDown={beginBatch}
                onPointerUp={commitBatch}
                onChange={(value) => updateSelectedSubtitles((subtitle) => ({
                  ...subtitle,
                  style: { ...subtitle.style, letterSpacing: value },
                }))}
              />
            </SettingRow>

            <label className="flex items-center gap-3 text-[10px] text-on-surface-variant cursor-pointer">
              <Checkbox
                checked={sourceSubtitle.style.background?.enabled ?? false}
                onChange={(e) => updateSelectedSubtitles((subtitle) => ({
                  ...subtitle,
                  style: {
                    ...subtitle.style,
                    background: {
                      enabled: e.target.checked,
                      color: subtitle.style.background?.color ?? '#000000',
                      opacity: subtitle.style.background?.opacity ?? 0.65,
                      paddingX: subtitle.style.background?.paddingX ?? 16,
                      paddingY: subtitle.style.background?.paddingY ?? 8,
                      borderRadius: subtitle.style.background?.borderRadius ?? 32,
                    },
                  },
                }))}
              />
              {t.backgroundPill}
            </label>
          </div>
        ) : null}
      </div>
    </PanelCard>
  );
}
