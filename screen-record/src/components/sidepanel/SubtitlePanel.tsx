import type { SubtitleMethod } from '@/hooks/useSubtitleGeneration';
import { AlignCenter } from 'lucide-react';
import { PanelCard } from '@/components/layout/PanelCard';
import { SettingRow } from '@/components/layout/SettingRow';
import { ColorPicker } from '@/components/ui/ColorPicker';
import { PanelSelect } from '@/components/ui/PanelSelect';
import { Slider } from '@/components/ui/Slider';
import { Checkbox } from '@/components/ui/checkbox';
import { useSettings } from '@/hooks/useSettings';
import { SUBTITLE_LANGUAGE_OPTIONS } from '@/lib/subtitleLanguageOptions';
import type { TrackSelectionRange } from '@/lib/timelineSegmentSelection';
import { VideoSegment } from '@/types/video';

export interface SubtitlePanelProps {
  segment: VideoSegment | null;
  selectedSubtitleIds?: string[];
  selectedSubtitleRange?: TrackSelectionRange | null;
  selectedSource: 'video' | 'mic';
  onSourceChange: (value: 'video' | 'mic') => void;
  selectedMethod: SubtitleMethod;
  onMethodChange: (value: SubtitleMethod) => void;
  methodCapabilities: Array<{ method: SubtitleMethod; available: boolean; reason?: string | null }>;
  canUseSelectedMethod: boolean;
  selectedMethodReason?: string | null;
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
  selectedSubtitleRange,
  selectedSource,
  onSourceChange,
  selectedMethod,
  onMethodChange,
  methodCapabilities,
  canUseSelectedMethod,
  selectedMethodReason,
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
  const subtitleActionDisabled = isGenerating || !hasSubtitleSource || !canUseSelectedMethod;
  const generateLabel = selectedSubtitleRange
    ? t.subtitleGenerateForRange
    : hasSubtitles
      ? t.subtitleRegenerate
      : t.subtitleGenerate;
  const isMultiSelect = (selectedSubtitleIds?.length ?? 0) >= 2;

  const getMethodLabel = (method: SubtitleMethod) => {
    switch (method) {
      case 'groq-whisper-large-v3-turbo':
        return t.subtitleMethodGroqWhisperLargeV3Turbo;
      case 'qwen-local-1-7b':
        return t.subtitleMethodQwenLocal1_7B;
      case 'qwen-local-0-6b':
        return t.subtitleMethodQwenLocal0_6B;
      case 'groq-whisper-accurate':
      default:
        return t.subtitleMethodGroqWhisperAccurate;
    }
  };

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

  const updateSubtitleText = (text: string) => {
    updateSelectedSubtitles((subtitle) => ({
      ...subtitle,
      text,
    }));
  };

  const subtitleSourceOptions = [
    {
      value: 'video',
      label: t.subtitleSourceVideo,
      disabled: !canUseVideoSource,
    },
    {
      value: 'mic',
      label: t.subtitleSourceMic,
      disabled: !canUseMicSource,
    },
  ];

  const subtitleMethodOptions = methodCapabilities.map((method) => ({
    value: method.method,
    label: getMethodLabel(method.method),
    disabled: !method.available,
  }));

  return (
    <PanelCard className="subtitle-panel">
      <div className="space-y-3.5">
        <p className="text-xs text-on-surface-variant">{t.subtitlePanelHint}</p>

        <div className="subtitle-source-row flex items-center gap-2">
          <span className="w-20 flex-shrink-0 text-[11px] font-medium text-on-surface-variant">
            {t.subtitleSource}
          </span>
          <PanelSelect
            value={selectedSource}
            options={subtitleSourceOptions}
            onChange={(value) => onSourceChange(value as 'video' | 'mic')}
            triggerClassName="subtitle-source-select flex-1"
            contentClassName="subtitle-source-menu"
          />
        </div>

        <div className="subtitle-method-row flex items-center gap-2">
          <span className="w-20 flex-shrink-0 text-[11px] font-medium text-on-surface-variant">
            {t.subtitleMethod}
          </span>
          <PanelSelect
            value={selectedMethod}
            options={subtitleMethodOptions}
            onChange={(value) => onMethodChange(value as SubtitleMethod)}
            triggerClassName="subtitle-method-select flex-1"
            contentClassName="subtitle-method-menu"
          />
        </div>

        <div className="subtitle-language-row flex items-center gap-2">
          <span className="w-20 flex-shrink-0 text-[11px] font-medium text-on-surface-variant">
            {t.subtitleLanguageHint}
          </span>
          <PanelSelect
            value={languageHint}
            options={SUBTITLE_LANGUAGE_OPTIONS}
            onChange={onLanguageHintChange}
            searchable
            searchPlaceholder={t.subtitleLanguageSearchPlaceholder}
            emptyStateLabel={t.subtitleLanguageSearchEmpty}
            triggerClassName="subtitle-language-select flex-1"
            contentClassName="subtitle-language-menu min-w-[max(300px,var(--radix-popover-trigger-width))]"
          />
        </div>

        <div className="subtitle-actions flex gap-2">
          <button
            type="button"
            disabled={subtitleActionDisabled}
            onClick={onGenerate}
            className="ui-button flex-1 rounded-xl px-3 py-2 text-sm font-medium disabled:opacity-50"
          >
            {generateLabel}
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
          {selectedMethodReason ?? statusMessage ?? (hasSubtitleSource ? t.subtitleIdleHint : t.subtitleUnavailableSource)}
        </p>

        {sourceSubtitle && editableSubtitles.length > 0 ? (
          <div className="subtitle-style-controls space-y-3.5">
            <div className="subtitle-badge-row flex items-center gap-2">
              <div
                className="subtitle-preview-badge inline-flex items-center gap-2 rounded-full px-3 py-1 text-[10px] font-medium"
                style={{ background: 'color-mix(in srgb, var(--timeline-zoom-color) 15%, transparent)', color: 'var(--timeline-zoom-color)' }}
              >
                <AlignCenter className="h-3 w-3" />
                {selection ? `${editableSubtitles.length} ${t.trackSubtitles}` : t.trackSubtitles}
              </div>

              {isMultiSelect ? (
                <div
                  className="subtitle-multi-select-badge rounded-md px-2 py-1 text-[10px] font-medium"
                  style={{ background: 'color-mix(in srgb, var(--timeline-zoom-color) 15%, transparent)', color: 'var(--timeline-zoom-color)' }}
                >
                  {selectedSubtitleIds!.length} {t.textMultiSelectLabel}
                </div>
              ) : null}
            </div>

            <textarea
              value={sourceSubtitle.text}
              onFocus={beginBatch}
              onBlur={commitBatch}
              onChange={(e) => updateSubtitleText(e.target.value)}
              className="subtitle-editor-input ui-input w-full rounded-xl px-3 py-2 text-on-surface text-sm thin-scrollbar subtle-resize"
              rows={2}
            />

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
