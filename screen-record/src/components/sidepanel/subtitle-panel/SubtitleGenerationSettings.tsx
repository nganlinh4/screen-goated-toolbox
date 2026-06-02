import type { SubtitleMethod } from '@/hooks/useSubtitleGeneration';
import type { SubtitleSource } from '@/lib/subtitleGenerationPlan';
import type { TrackSelectionRange } from '@/lib/timelineSegmentSelection';
import type { ImportedAudioSegment } from '@/types/video';
import type { Translations } from '@/i18n';
import { useState } from 'react';
import { Plus, RotateCcw, Trash2, X } from 'lucide-react';
import type { PanelSelectOption } from '@/components/ui/PanelSelect';
import { PanelSelect } from '@/components/ui/PanelSelect';
import { Slider } from '@/components/ui/Slider';
import { Checkbox } from '@/components/ui/checkbox';
import { SettingRow } from '@/components/layout/SettingRow';
import type { useSubtitleTranslation } from '@/hooks/useSubtitleTranslation';
import {
  DEFAULT_GEMINI_SUBTITLE_PROMPT,
  GEMINI_SUBTITLE_OUTPUT_CONTRACT_PREVIEW,
  GEMINI_SUBTITLE_PROMPT_PRESETS,
} from '@/lib/geminiSubtitlePrompt';
import { getSubtitleLanguageOptionsForMethod } from '@/lib/subtitleLanguageOptions';
import { getSubtitleTrackLabel, ORIGINAL_SUBTITLE_TRACK_ID } from '@/lib/subtitleTracks';
import {
  buildSubtitleMethodOptions,
  subtitleMethodUsesGeminiPrompt,
  subtitleMethodUsesGroqVocabulary,
  subtitleMethodUsesLanguageHint,
} from './subtitleMethodHelp';

interface SubtitleGenerationSettingsProps {
  t: Translations;
  visibleSubtitleCount: number;
  selectedSubtitleRange?: TrackSelectionRange | null;
  selectedSource: SubtitleSource;
  onSourceChange: (value: SubtitleSource) => void;
  selectedMethod: SubtitleMethod;
  onMethodChange: (value: SubtitleMethod) => void;
  methodCapabilities: Array<{ method: SubtitleMethod; available: boolean; reason?: string | null }>;
  canUseSelectedMethod: boolean;
  selectedMethodReason?: string | null;
  languageHint: string;
  onLanguageHintChange: (value: string) => void;
  geminiPrompt: string;
  onGeminiPromptChange: (value: string) => void;
  groqVocabulary: string[];
  onGroqVocabularyChange: (value: string[]) => void;
  autoSplitSubtitles: boolean;
  onAutoSplitSubtitlesChange: (value: boolean) => void;
  autoSplitSubtitleMaxUnits: number;
  onAutoSplitSubtitleMaxUnitsChange: (value: number) => void;
  isGenerating: boolean;
  statusMessage?: string | null;
  canUseVideoSource: boolean;
  canUseMicSource: boolean;
  canUseAudioSource: boolean;
  audioSegments: ImportedAudioSegment[];
  onGenerate: () => void;
  onCancel: () => void;
  canExportSrt: boolean;
  onExportSrt: () => void;
  canExportAudioSrt: boolean;
  onExportAudioSrt: () => void;
  subtitleTranslation: ReturnType<typeof useSubtitleTranslation>;
}

export function SubtitleGenerationSettings({
  t,
  visibleSubtitleCount,
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
  geminiPrompt,
  onGeminiPromptChange,
  groqVocabulary,
  onGroqVocabularyChange,
  autoSplitSubtitles,
  onAutoSplitSubtitlesChange,
  autoSplitSubtitleMaxUnits,
  onAutoSplitSubtitleMaxUnitsChange,
  isGenerating,
  statusMessage,
  canUseVideoSource,
  canUseMicSource,
  canUseAudioSource,
  audioSegments,
  onGenerate,
  onCancel,
  canExportSrt,
  onExportSrt,
  canExportAudioSrt,
  onExportAudioSrt,
  subtitleTranslation,
}: SubtitleGenerationSettingsProps) {
  const [pendingGroqVocabulary, setPendingGroqVocabulary] = useState('');
  const hasSubtitleSource = canUseVideoSource || canUseMicSource || canUseAudioSource;
  const hasSubtitles = visibleSubtitleCount > 0;
  const subtitleActionDisabled = isGenerating
    || !hasSubtitleSource
    || !canUseSelectedMethod
    || !subtitleTranslation.canGenerateSubtitlesFromCurrentView;
  const generateLabel = selectedSubtitleRange
    ? t.subtitleGenerateForRange
    : hasSubtitles
      ? t.subtitleRegenerate
      : t.subtitleGenerate;
  const usesLanguageHint = subtitleMethodUsesLanguageHint(selectedMethod);
  const usesGeminiPrompt = subtitleMethodUsesGeminiPrompt(selectedMethod);
  const usesGroqVocabulary = subtitleMethodUsesGroqVocabulary(selectedMethod);
  const languageOptions = getSubtitleLanguageOptionsForMethod(selectedMethod);
  const subtitleStatusText = selectedMethodReason
    ?? statusMessage
    ?? (hasSubtitleSource ? t.subtitleIdleHint : t.subtitleUnavailableSource);
  const subtitleSourceOptions: PanelSelectOption[] = [
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
    {
      value: 'audio',
      label: t.subtitleSourceFullAudio,
      disabled: !canUseAudioSource,
    },
    ...audioSegments.map((segment) => ({
      value: `audio:${segment.id}`,
      label: segment.name || t.subtitleSourceAudio,
      disabled: !segment.rawAudioPath,
    })),
  ];
  const subtitleMethodOptions = buildSubtitleMethodOptions(t, methodCapabilities);
  const subtitleViewOptions: PanelSelectOption[] = [
    {
      value: ORIGINAL_SUBTITLE_TRACK_ID,
      label: t.subtitleTrackOriginal,
    },
    ...subtitleTranslation.subtitleTracks
      .filter((track) => track.kind === 'translation')
      .map((track) => ({
        value: track.id,
        label: getSubtitleTrackLabel(track),
        action: {
          label: t.subtitleTrackDelete,
          icon: <Trash2 className="h-3.5 w-3.5" />,
          tone: 'danger' as const,
          onClick: () => subtitleTranslation.deleteSubtitleTrack(track.id),
        },
      })),
    {
      value: 'custom',
      label: t.subtitleTrackCustom,
    },
  ];

  const addGroqVocabulary = () => {
    const value = pendingGroqVocabulary.trim();
    if (!value) return;
    const exists = groqVocabulary.some((entry) => entry.toLocaleLowerCase() === value.toLocaleLowerCase());
    if (!exists) {
      onGroqVocabularyChange([...groqVocabulary, value]);
    }
    setPendingGroqVocabulary('');
  };

  const removeGroqVocabulary = (value: string) => {
    onGroqVocabularyChange(groqVocabulary.filter((entry) => entry !== value));
  };

  return (
    <>
      <div className="subtitle-view-row flex items-center gap-2">
        <span className="w-20 flex-shrink-0 text-[11px] font-medium text-on-surface-variant">
          {t.subtitleTrackView}
        </span>
        <PanelSelect
          value={subtitleTranslation.activeSubtitleView.kind === 'custom'
            ? 'custom'
            : subtitleTranslation.activeSubtitleView.trackId ?? ORIGINAL_SUBTITLE_TRACK_ID}
          options={subtitleViewOptions}
          onChange={subtitleTranslation.setSubtitleView}
          triggerClassName="subtitle-view-select h-8 flex-1 rounded-lg px-2.5 text-[11px]"
          contentClassName="subtitle-view-menu"
        />
      </div>

      <div className="subtitle-source-row flex items-center gap-2">
        <span className="w-20 flex-shrink-0 text-[11px] font-medium text-on-surface-variant">
          {t.subtitleSource}
        </span>
        <PanelSelect
          value={selectedSource}
          options={subtitleSourceOptions}
          onChange={(value) => onSourceChange(value as SubtitleSource)}
          triggerClassName="subtitle-source-select h-8 flex-1 rounded-lg px-2.5 text-[11px]"
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
          triggerClassName="subtitle-method-select h-8 flex-1 rounded-lg px-2.5 text-[11px]"
          contentClassName="subtitle-method-menu"
        />
      </div>

      {usesLanguageHint && (
        <div className="subtitle-language-row flex items-center gap-2">
          <span className="w-20 flex-shrink-0 text-[11px] font-medium text-on-surface-variant">
            {t.subtitleLanguageHint}
          </span>
          <PanelSelect
            value={languageHint}
            options={languageOptions}
            onChange={onLanguageHintChange}
            searchable
            searchPlaceholder={t.subtitleLanguageSearchPlaceholder}
            emptyStateLabel={t.subtitleLanguageSearchEmpty}
            triggerClassName="subtitle-language-select h-8 flex-1 rounded-lg px-2.5 text-[11px]"
            contentClassName="subtitle-language-menu"
          />
        </div>
      )}

      {usesGeminiPrompt && (
        <div className="subtitle-gemini-prompt-row space-y-2.5">
          <div className="subtitle-gemini-prompt-header flex items-center justify-between gap-2">
            <div className="subtitle-gemini-prompt-label text-[11px] font-medium text-on-surface-variant">
              {t.subtitleGeminiPrompt}
            </div>
            <button
              type="button"
              onClick={() => onGeminiPromptChange(DEFAULT_GEMINI_SUBTITLE_PROMPT)}
              className="subtitle-gemini-prompt-reset ui-chip-button inline-flex h-6 items-center gap-1 rounded-md px-2 text-[10px] font-medium"
              title={t.subtitleGeminiPromptReset}
            >
              <RotateCcw className="h-3 w-3" />
              {t.subtitleGeminiPromptReset}
            </button>
          </div>
          <div className="subtitle-gemini-prompt-presets flex flex-wrap gap-1.5">
            {GEMINI_SUBTITLE_PROMPT_PRESETS.map((preset) => {
              const isActive = geminiPrompt.trim() === preset.prompt.trim();
              return (
                <button
                  key={preset.id}
                  type="button"
                  onClick={() => onGeminiPromptChange(preset.prompt)}
                  className={`subtitle-gemini-prompt-preset ui-chip-button rounded-full px-2 py-1 text-[10px] font-medium ${
                    isActive ? 'ui-chip-button-active' : ''
                  }`}
                >
                  {t[preset.labelKey]}
                </button>
              );
            })}
          </div>
          <div className="subtitle-gemini-prompt-editor ui-input thin-scrollbar overflow-hidden rounded-xl p-0">
            <textarea
              value={geminiPrompt}
              onChange={(event) => onGeminiPromptChange(event.target.value)}
              placeholder={t.subtitleGeminiPromptPlaceholder}
              rows={7}
              className="subtitle-gemini-prompt-input subtle-resize min-h-[132px] w-full resize-y border-0 bg-transparent px-3 py-2.5 text-[11px] leading-4 text-on-surface outline-none"
            />
            <div
              aria-readonly="true"
              className="subtitle-gemini-prompt-contract whitespace-pre-wrap border-t border-outline/25 bg-surface-container-highest/45 px-3 py-2.5 text-[10px] leading-4 text-on-surface-variant opacity-65"
            >
              {GEMINI_SUBTITLE_OUTPUT_CONTRACT_PREVIEW}
            </div>
          </div>
        </div>
      )}

      {usesGroqVocabulary && (
        <div className="subtitle-groq-vocabulary-row space-y-2">
          <div className="subtitle-groq-vocabulary-label text-[11px] font-medium text-on-surface-variant">
            {t.subtitleGroqVocabulary}
          </div>
          <div className="subtitle-groq-vocabulary-input-row flex items-center gap-1.5">
            <input
              type="text"
              value={pendingGroqVocabulary}
              onChange={(event) => setPendingGroqVocabulary(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Enter') {
                  event.preventDefault();
                  addGroqVocabulary();
                }
              }}
              placeholder={t.subtitleGroqVocabularyPlaceholder}
              className="subtitle-groq-vocabulary-input ui-input h-8 min-w-0 flex-1 rounded-lg px-2.5 text-[11px]"
            />
            <button
              type="button"
              onClick={addGroqVocabulary}
              className="subtitle-groq-vocabulary-add ui-chip-button flex h-8 w-8 items-center justify-center rounded-lg"
              title={t.subtitleGroqVocabularyAdd}
            >
              <Plus className="h-3.5 w-3.5" />
            </button>
          </div>
          {groqVocabulary.length > 0 && (
            <div className="subtitle-groq-vocabulary-tags flex flex-wrap gap-1.5">
              {groqVocabulary.map((entry) => (
                <button
                  key={entry}
                  type="button"
                  onClick={() => removeGroqVocabulary(entry)}
                  className="subtitle-groq-vocabulary-tag ui-chip-button inline-flex max-w-full items-center gap-1 rounded-full px-2 py-1 text-[10px]"
                  title={t.subtitleGroqVocabularyRemove}
                >
                  <span className="subtitle-groq-vocabulary-tag-text truncate">{entry}</span>
                  <X className="h-3 w-3 flex-shrink-0" />
                </button>
              ))}
            </div>
          )}
        </div>
      )}

      <div className="subtitle-auto-split-row rounded-lg border border-outline/30 bg-surface-container-high/40 p-2">
        <label className="subtitle-auto-split-toggle flex cursor-pointer items-center gap-2 text-[11px] font-medium text-on-surface">
          <Checkbox
            checked={autoSplitSubtitles}
            onChange={(event) => onAutoSplitSubtitlesChange(event.target.checked)}
          />
          {t.subtitleAutoSplit}
        </label>
        {autoSplitSubtitles ? (
          <div className="subtitle-auto-split-controls mt-2 space-y-1.5">
            <SettingRow
              label={t.smartSplitMaxWords}
              valueDisplay={`${autoSplitSubtitleMaxUnits}`}
              className="subtitle-auto-split-max-words-row"
            >
              <Slider
                min={3}
                max={24}
                step={1}
                value={autoSplitSubtitleMaxUnits}
                onChange={onAutoSplitSubtitleMaxUnitsChange}
              />
            </SettingRow>
            <p className="subtitle-auto-split-hint text-[10px] leading-4 text-on-surface-variant">
              {t.subtitleAutoSplitHint}
            </p>
          </div>
        ) : null}
      </div>

      <div className="subtitle-actions grid grid-cols-2 gap-1.5">
        <button
          type="button"
          disabled={subtitleActionDisabled}
          onClick={onGenerate}
          data-tone="primary"
          data-emphasis="strong"
          className="subtitle-generate-button ui-action-button flex h-8 items-center justify-center rounded-lg px-2.5 text-[11px] font-medium leading-tight"
        >
          {generateLabel}
        </button>
        <button
          type="button"
          disabled={!isGenerating}
          onClick={onCancel}
          data-tone="danger"
          className="subtitle-cancel-button ui-action-button flex h-8 items-center justify-center rounded-lg px-2.5 text-[11px] font-medium leading-tight"
        >
          {t.subtitleCancelJob}
        </button>
        <button
          type="button"
          disabled={!canExportSrt}
          onClick={onExportSrt}
          data-tone="success"
          className="subtitle-export-srt-button ui-action-button flex h-8 items-center justify-center rounded-lg px-2.5 text-[11px] font-medium leading-tight"
        >
          {selectedSubtitleRange ? t.subtitleExportRangeSrt : t.subtitleExportSrt}
        </button>
        <button
          type="button"
          disabled={!canExportAudioSrt}
          onClick={onExportAudioSrt}
          data-tone="success"
          className="subtitle-export-audio-srt-button ui-action-button flex h-8 items-center justify-center rounded-lg px-2.5 text-[11px] font-medium leading-tight"
        >
          {t.subtitleExportAudioSrt}
        </button>
      </div>

      <p className="subtitle-status-message text-[11px] leading-4 text-on-surface-variant">
        {subtitleStatusText}
      </p>
    </>
  );
}
