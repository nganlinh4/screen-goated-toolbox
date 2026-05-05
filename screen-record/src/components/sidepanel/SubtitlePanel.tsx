import type { SubtitleMethod } from '@/hooks/useSubtitleGeneration';
import type { SubtitleSource } from '@/lib/subtitleGenerationPlan';
import { useState } from 'react';
import { AlignCenter, AlignLeft, AlignRight, HelpCircle, Plus, RotateCcw, Trash2, X } from 'lucide-react';
import { PanelCard } from '@/components/layout/PanelCard';
import { SettingRow } from '@/components/layout/SettingRow';
import { ColorPicker } from '@/components/ui/ColorPicker';
import { PanelSelect } from '@/components/ui/PanelSelect';
import { Slider } from '@/components/ui/Slider';
import { Tooltip } from '@/components/ui/Tooltip';
import { Checkbox } from '@/components/ui/checkbox';
import { useSettings } from '@/hooks/useSettings';
import { useSubtitleTranslation } from '@/hooks/useSubtitleTranslation';
import {
  DEFAULT_GEMINI_SUBTITLE_PROMPT,
  GEMINI_SUBTITLE_PROMPT_PRESETS,
  GEMINI_SUBTITLE_OUTPUT_CONTRACT_PREVIEW,
} from '@/lib/geminiSubtitlePrompt';
import { getSubtitleLanguageOptionsForMethod } from '@/lib/subtitleLanguageOptions';
import { normalizeTextStyle } from '@/lib/textStyleDefaults';
import type { TrackSelectionRange } from '@/lib/timelineSegmentSelection';
import {
  updateSubtitleStylesAcrossTracks,
  updateSubtitleTextsOnActiveTrack,
} from '@/lib/subtitleTrackMutations';
import { getSubtitleTrackLabel, ORIGINAL_SUBTITLE_TRACK_ID } from '@/lib/subtitleTracks';
import type { ImportedAudioSegment, VideoSegment } from '@/types/video';
import { SubtitleCustomChainEditor } from './subtitle-panel/SubtitleCustomChainEditor';

const METHOD_LANGUAGE_PREVIEW_LIMIT = 14;

const LOCAL_SUBTITLE_METHODS = new Set<SubtitleMethod>([
  'qwen-local-0-6b',
  'qwen-local-1-7b',
  'parakeet-tdt-0-6b-v3',
]);

const PARAKEET_TDT_0_6B_V3_LANGUAGES = [
  'Bulgarian',
  'Croatian',
  'Czech',
  'Danish',
  'Dutch',
  'English',
  'Estonian',
  'Finnish',
  'French',
  'German',
  'Greek',
  'Hungarian',
  'Italian',
  'Latvian',
  'Lithuanian',
  'Maltese',
  'Polish',
  'Portuguese',
  'Romanian',
  'Slovak',
  'Slovenian',
  'Spanish',
  'Swedish',
  'Russian',
  'Ukrainian',
];

function buildFontVariationCSS(vars?: NonNullable<VideoSegment['subtitleSegments']>[number]['style']['fontVariations']): string | undefined {
  const parts: string[] = [];
  if (vars?.wdth !== undefined && vars.wdth !== 100) parts.push(`'wdth' ${vars.wdth}`);
  if (vars?.slnt !== undefined && vars.slnt !== 0) parts.push(`'slnt' ${vars.slnt}`);
  if (vars?.ROND !== undefined && vars.ROND !== 0) parts.push(`'ROND' ${vars.ROND}`);
  return parts.length > 0 ? parts.join(', ') : undefined;
}

function summarizeLanguageSupport(
  method: SubtitleMethod,
  t: ReturnType<typeof useSettings>['t'],
): string {
  if (method === 'gemini-3-1-flash-lite' || method === 'gemini-3-flash-preview') {
    return t.subtitleMethodHelpGeminiLanguages;
  }

  if (method === 'parakeet-tdt-0-6b-v3') {
    return `${t.subtitleMethodHelpParakeetLanguages}: ${PARAKEET_TDT_0_6B_V3_LANGUAGES.join(', ')}`;
  }

  const languageLabels = getSubtitleLanguageOptionsForMethod(method)
    .filter((option) => option.value !== 'auto')
    .map((option) => option.label);
  const preview = languageLabels.slice(0, METHOD_LANGUAGE_PREVIEW_LIMIT).join(', ');
  const remaining = languageLabels.length - METHOD_LANGUAGE_PREVIEW_LIMIT;

  if (remaining <= 0) {
    return preview || t.subtitleMethodHelpUnknownLanguages;
  }

  const prefix = method === 'qwen-local-0-6b' || method === 'qwen-local-1-7b'
    ? t.subtitleMethodHelpQwenLanguages
    : t.subtitleMethodHelpGroqLanguages;

  return `${prefix}: ${preview}, ${t.subtitleMethodHelpMoreLanguages.replace('{count}', String(remaining))}`;
}

export interface SubtitlePanelProps {
  segment: VideoSegment | null;
  editingSubtitleId: string | null;
  selectedSubtitleIds?: string[];
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
  isGenerating: boolean;
  statusMessage?: string | null;
  canUseVideoSource: boolean;
  canUseMicSource: boolean;
  canUseAudioSource: boolean;
  audioSegments?: ImportedAudioSegment[];
  onGenerate: () => void;
  onCancel: () => void;
  canExportSrt: boolean;
  onExportSrt: () => void;
  canExportAudioSrt: boolean;
  onExportAudioSrt: () => void;
  subtitleTranslation: ReturnType<typeof useSubtitleTranslation>;
  onUpdateSegment: (segment: VideoSegment) => void;
  beginBatch: () => void;
  commitBatch: () => void;
}

export function SubtitlePanel({
  segment,
  editingSubtitleId,
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
  geminiPrompt,
  onGeminiPromptChange,
  groqVocabulary,
  onGroqVocabularyChange,
  isGenerating,
  statusMessage,
  canUseVideoSource,
  canUseMicSource,
  canUseAudioSource,
  audioSegments = [],
  onGenerate,
  onCancel,
  canExportSrt,
  onExportSrt,
  canExportAudioSrt,
  onExportAudioSrt,
  subtitleTranslation,
  onUpdateSegment,
  beginBatch,
  commitBatch,
}: SubtitlePanelProps) {
  const { t } = useSettings();
  const visibleSubtitles = subtitleTranslation.visibleSubtitleSegments;
  const hasSelection = (selectedSubtitleIds?.length ?? 0) > 0;
  const selection = hasSelection ? new Set(selectedSubtitleIds) : null;
  const sourceId = hasSelection ? selectedSubtitleIds![0] : editingSubtitleId;
  const sourceSubtitle = sourceId
    ? visibleSubtitles.find((subtitle) => subtitle.id === sourceId) ?? null
    : null;
  const resolvedStyle = sourceSubtitle ? normalizeTextStyle(sourceSubtitle.style) : null;
  const editableSubtitles = selection
    ? visibleSubtitles.filter((subtitle) => selection.has(subtitle.id))
    : sourceSubtitle
      ? [sourceSubtitle]
      : [];
  const hasSubtitleSource = canUseVideoSource || canUseMicSource || canUseAudioSource;
  const hasSubtitles = visibleSubtitles.length > 0;
  const subtitleActionDisabled = isGenerating
    || !hasSubtitleSource
    || !canUseSelectedMethod
    || !subtitleTranslation.canGenerateSubtitlesFromCurrentView;
  const generateLabel = selectedSubtitleRange
    ? t.subtitleGenerateForRange
    : hasSubtitles
      ? t.subtitleRegenerate
      : t.subtitleGenerate;
  const isMultiSelect = (selectedSubtitleIds?.length ?? 0) >= 2;
  const usesLanguageHint = !(
    selectedMethod === 'gemini-3-1-flash-lite'
    || selectedMethod === 'gemini-3-flash-preview'
    || selectedMethod === 'parakeet-tdt-0-6b-v3'
  );
  const usesGeminiPrompt = (
    selectedMethod === 'gemini-3-1-flash-lite'
    || selectedMethod === 'gemini-3-flash-preview'
  );
  const usesGroqVocabulary = (
    selectedMethod === 'groq-whisper-accurate'
    || selectedMethod === 'groq-whisper-large-v3-turbo'
  );
  const languageOptions = getSubtitleLanguageOptionsForMethod(selectedMethod);
  const [pendingGroqVocabulary, setPendingGroqVocabulary] = useState('');

  function getMethodLabel(method: SubtitleMethod) {
    switch (method) {
      case 'groq-whisper-large-v3-turbo':
        return t.subtitleMethodGroqWhisperLargeV3Turbo;
      case 'gemini-3-1-flash-lite':
        return t.subtitleMethodGemini3_1FlashLite;
      case 'gemini-3-flash-preview':
        return t.subtitleMethodGemini3FlashPreview;
      case 'qwen-local-1-7b':
        return t.subtitleMethodQwenLocal1_7B;
      case 'qwen-local-0-6b':
        return t.subtitleMethodQwenLocal0_6B;
      case 'parakeet-tdt-0-6b-v3':
        return t.subtitleMethodParakeetTdt0_6BV3;
      case 'groq-whisper-accurate':
      default:
        return t.subtitleMethodGroqWhisperAccurate;
    }
  }

  function getMethodHelpContent(method: SubtitleMethod) {
    const runtime = LOCAL_SUBTITLE_METHODS.has(method)
      ? t.subtitleMethodHelpRuntimeLocal
      : t.subtitleMethodHelpRuntimeCloud;

    return (
      <div className="subtitle-method-help-tooltip max-w-[280px] space-y-1 text-left leading-4">
        <div className="font-semibold text-[var(--on-surface)]">{getMethodLabel(method)}</div>
        <div>
          <span className="text-[var(--on-surface-variant)]">{t.subtitleMethodHelpRuntime}: </span>
          <span>{runtime}</span>
        </div>
        <div>
          <span className="text-[var(--on-surface-variant)]">{t.subtitleMethodHelpLanguages}: </span>
          <span>{summarizeLanguageSupport(method, t)}</span>
        </div>
      </div>
    );
  }

  const updateSelectedSubtitles = (updater: (subtitle: NonNullable<typeof sourceSubtitle>) => NonNullable<typeof sourceSubtitle>) => {
    if (!segment || !sourceSubtitle) return;
    const targetIds = selection ? selection : new Set([sourceSubtitle.id]);
    onUpdateSegment(updateSubtitleStylesAcrossTracks(segment, targetIds, updater));
  };

  const updateSubtitleText = (text: string) => {
    if (!segment || !sourceSubtitle || subtitleTranslation.isCustomSubtitleView) return;
    const targetIds = selection ? selection : new Set([sourceSubtitle.id]);
    onUpdateSegment(updateSubtitleTextsOnActiveTrack(segment, targetIds, (subtitle) => ({
      ...subtitle,
      text,
    })));
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

  const subtitleMethodOptions = methodCapabilities.map((method) => ({
    value: method.method,
    label: getMethodLabel(method.method),
    trailing: (
      <Tooltip content={getMethodHelpContent(method.method)} side="left" delayDuration={150}>
        <button
          type="button"
          className="subtitle-method-option-help flex h-6 w-6 items-center justify-center rounded-md text-on-surface-variant transition-colors hover:bg-[color-mix(in_srgb,var(--primary-color)_12%,transparent)] hover:text-[var(--primary-color)]"
          aria-label={`${t.subtitleMethodHelpLabel}: ${getMethodLabel(method.method)}`}
        >
          <HelpCircle className="h-3.5 w-3.5" />
        </button>
      </Tooltip>
    ),
    disabled: !method.available
      && !(
        (
          method.method === 'qwen-local-0-6b'
          || method.method === 'qwen-local-1-7b'
          || method.method === 'parakeet-tdt-0-6b-v3'
        )
        && method.reason?.includes('Downloaded Tools')
      ),
  }));
  const subtitleTranslationSourceOptions = [
    {
      value: 'current',
      label: t.subtitleTranslationSourceCurrent,
      disabled: (subtitleTranslation.subtitleTranslationSourceCounts.current ?? 0) <= 0,
    },
    {
      value: 'all',
      label: t.subtitleTranslationSourceAll,
      disabled: (subtitleTranslation.subtitleTranslationSourceCounts.all ?? 0) <= 0,
    },
    {
      value: 'audio',
      label: t.subtitleTranslationSourceAllAudio,
      disabled: (subtitleTranslation.subtitleTranslationSourceCounts.audio ?? 0) <= 0,
    },
    {
      value: 'video',
      label: t.subtitleSourceVideo,
      disabled: (subtitleTranslation.subtitleTranslationSourceCounts.video ?? 0) <= 0,
    },
    {
      value: 'mic',
      label: t.subtitleSourceMic,
      disabled: (subtitleTranslation.subtitleTranslationSourceCounts.mic ?? 0) <= 0,
    },
  ];
  const subtitleViewOptions = [
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
  const subtitleStatusText = selectedMethodReason
    ?? statusMessage
    ?? (hasSubtitleSource ? t.subtitleIdleHint : t.subtitleUnavailableSource);
  const translationStatusText = subtitleTranslation.subtitleTranslationStatusMessage
    ?? subtitleTranslation.subtitleTranslationCapabilities?.reason
    ?? t.subtitleTranslationHint;

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
    <PanelCard className="subtitle-panel">
      <div className="subtitle-panel-body space-y-3">
        <p className="subtitle-panel-hint text-[11px] leading-4 text-on-surface-variant">{t.subtitlePanelHint}</p>

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

        <div className="subtitle-translation-language-row flex items-center gap-2">
          <span className="w-20 flex-shrink-0 text-[11px] font-medium text-on-surface-variant">
            {t.subtitleTranslationLanguage}
          </span>
          <PanelSelect
            value={subtitleTranslation.subtitleTranslationTargetLanguage}
            options={subtitleTranslation.subtitleTranslationLanguageOptions}
            onChange={subtitleTranslation.setSubtitleTranslationTargetLanguage}
            searchable
            searchPlaceholder={t.subtitleLanguageSearchPlaceholder}
            emptyStateLabel={t.subtitleLanguageSearchEmpty}
            triggerClassName="subtitle-translation-language-select h-8 flex-1 rounded-lg px-2.5 text-[11px]"
            contentClassName="subtitle-translation-language-menu"
          />
        </div>

        <div className="subtitle-translation-source-row flex items-center gap-2">
          <span className="w-20 flex-shrink-0 text-[11px] font-medium text-on-surface-variant">
            {t.subtitleTranslationSource}
          </span>
          <PanelSelect
            value={subtitleTranslation.subtitleTranslationSource}
            options={subtitleTranslationSourceOptions}
            onChange={(value) =>
              subtitleTranslation.setSubtitleTranslationSource(
                value as typeof subtitleTranslation.subtitleTranslationSource,
              )
            }
            triggerClassName="subtitle-translation-source-select h-8 flex-1 rounded-lg px-2.5 text-[11px]"
            contentClassName="subtitle-translation-source-menu"
          />
        </div>

        <div className="subtitle-translation-chunk-row space-y-1.5">
          <div className="subtitle-translation-chunk-header flex items-center justify-between gap-2">
            <span className="text-[11px] font-medium text-on-surface-variant">
              {t.subtitleTranslationChunking}
            </span>
            <span className="subtitle-translation-chunk-value text-[10px] font-semibold text-on-surface-variant">
              {subtitleTranslation.subtitleTranslationChunkCount}/{subtitleTranslation.subtitleTranslationChunkMax}
            </span>
          </div>
          <Slider
            min={1}
            max={subtitleTranslation.subtitleTranslationChunkMax}
            step={1}
            value={subtitleTranslation.subtitleTranslationChunkCount}
            onChange={subtitleTranslation.setSubtitleTranslationChunkCount}
            onPointerDown={() => subtitleTranslation.setSubtitleTranslationChunkDragging(true)}
            onPointerUp={() => subtitleTranslation.setSubtitleTranslationChunkDragging(false)}
            onPointerCancel={() => subtitleTranslation.setSubtitleTranslationChunkDragging(false)}
            onBlur={() => subtitleTranslation.setSubtitleTranslationChunkDragging(false)}
            className="subtitle-translation-chunk-slider"
            disabled={subtitleTranslation.subtitleTranslationChunkMax <= 1}
          />
        </div>

        <div className="subtitle-translation-instructions-row space-y-1.5">
          <div className="subtitle-translation-instructions-label text-[11px] font-medium text-on-surface-variant">
            {t.subtitleTranslationInstructions}
          </div>
          <textarea
            value={subtitleTranslation.subtitleTranslationInstructions}
            onChange={(event) => subtitleTranslation.setSubtitleTranslationInstructions(event.target.value)}
            placeholder={t.subtitleTranslationInstructionsPlaceholder}
            rows={4}
            className="subtitle-translation-instructions-input ui-input thin-scrollbar subtle-resize min-h-[96px] w-full rounded-xl px-3 py-2.5 text-[11px] leading-4 text-on-surface"
          />
        </div>

        <div className="subtitle-translation-actions grid grid-cols-2 gap-1.5">
          <button
            type="button"
            disabled={!subtitleTranslation.canTranslateSubtitles || subtitleTranslation.isTranslatingSubtitles}
            onClick={subtitleTranslation.handleTranslateSubtitles}
            data-tone="primary"
            className="subtitle-translate-button ui-action-button flex h-8 items-center justify-center rounded-lg px-2.5 text-[11px] font-medium leading-tight"
          >
            {subtitleTranslation.hasExistingTranslationForTargetLanguage
              ? t.subtitleTranslateUpdate
              : t.subtitleTranslate}
          </button>
          <button
            type="button"
            disabled={!subtitleTranslation.isTranslatingSubtitles}
            onClick={subtitleTranslation.handleCancelSubtitleTranslation}
            data-tone="danger"
            className="subtitle-translate-cancel-button ui-action-button flex h-8 items-center justify-center rounded-lg px-2.5 text-[11px] font-medium leading-tight"
          >
            {t.subtitleCancelJob}
          </button>
        </div>

        <p className="subtitle-translation-status text-[11px] leading-4 text-on-surface-variant">
          {translationStatusText}
        </p>

        {subtitleTranslation.isCustomSubtitleView ? (
          <SubtitleCustomChainEditor
            t={t}
            tracks={subtitleTranslation.subtitleTracks}
            chain={subtitleTranslation.subtitleCustomChain}
            onChange={subtitleTranslation.updateSubtitleCustomChain}
          />
        ) : null}

        {sourceSubtitle && editableSubtitles.length > 0 && resolvedStyle ? (
          <div className="subtitle-style-controls space-y-3">
            <div className="subtitle-badge-row flex items-center gap-1.5">
              <div
                className="subtitle-preview-badge inline-flex items-center gap-1.5 rounded-full px-2.5 py-1 text-[10px] font-medium"
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
              className="subtitle-editor-input ui-input w-full rounded-xl px-3 py-2 text-sm text-on-surface thin-scrollbar subtle-resize"
              disabled={subtitleTranslation.isCustomSubtitleView}
              style={{
                fontFamily: "'Google Sans Flex', 'Segoe UI', system-ui, sans-serif",
                fontWeight: resolvedStyle.fontVariations?.wght ?? 400,
                fontVariationSettings: buildFontVariationCSS(resolvedStyle.fontVariations),
              }}
              rows={2}
            />

            <p className="text-[10px] text-on-surface-variant">
              {subtitleTranslation.isCustomSubtitleView ? t.subtitleCustomReadOnly : t.dragTextHint}
            </p>

            <SettingRow label={t.fontSize} valueDisplay={`${resolvedStyle.fontSize}`}>
              <Slider
                min={12}
                max={200}
                step={1}
                value={resolvedStyle.fontSize}
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
                value={resolvedStyle.color}
                onChange={(color) => updateSelectedSubtitles((subtitle) => ({
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
                    onChange={(nextValue) => updateSelectedSubtitles((subtitle) => ({
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
                      onClick={() => updateSelectedSubtitles((subtitle) => ({
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
                onChange={(value) => updateSelectedSubtitles((subtitle) => ({
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
                onChange={(value) => updateSelectedSubtitles((subtitle) => ({
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
                onChange={(value) => updateSelectedSubtitles((subtitle) => ({
                  ...subtitle,
                  style: { ...subtitle.style, lineHeight: value },
                }))}
              />
            </SettingRow>

            <div>
              <label className="subtitle-wrap-toggle flex items-center gap-3 text-[11px] text-on-surface-variant cursor-pointer">
                <Checkbox
                  checked={resolvedStyle.wrap?.enabled ?? true}
                  onChange={(e) => updateSelectedSubtitles((subtitle) => ({
                    ...subtitle,
                    style: {
                      ...subtitle.style,
                      wrap: {
                        ...(resolvedStyle.wrap ?? { enabled: true, maxWidthPercent: 80 }),
                        enabled: e.target.checked,
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
                      onChange={(value) => updateSelectedSubtitles((subtitle) => ({
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
                onChange={(value) => updateSelectedSubtitles((subtitle) => ({
                  ...subtitle,
                  style: {
                    ...subtitle.style,
                    animation: { ...(resolvedStyle.animation ?? { preset: 'fade', inDuration: 0.3, outDuration: 0.3 }), preset: value as any },
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
                    onChange={(value) => updateSelectedSubtitles((subtitle) => ({
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
                    onChange={(value) => updateSelectedSubtitles((subtitle) => ({
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
                onChange={(e) => updateSelectedSubtitles((subtitle) => ({
                  ...subtitle,
                  style: {
                    ...subtitle.style,
                    background: {
                      enabled: e.target.checked,
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
                    onChange={(color) => updateSelectedSubtitles((subtitle) => ({
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
                    onChange={(value) => updateSelectedSubtitles((subtitle) => ({
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
                    onChange={(value) => updateSelectedSubtitles((subtitle) => ({
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
                    onChange={(value) => updateSelectedSubtitles((subtitle) => ({
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
                    onChange={(value) => updateSelectedSubtitles((subtitle) => ({
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
                  onChange={(e) => updateSelectedSubtitles((subtitle) => ({
                    ...subtitle,
                    style: {
                      ...subtitle.style,
                      stroke: { ...(resolvedStyle.stroke ?? { enabled: false, color: '#000000', width: 2, opacity: 1 }), enabled: e.target.checked },
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
                      onChange={(color) => updateSelectedSubtitles((subtitle) => ({
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
                      onChange={(value) => updateSelectedSubtitles((subtitle) => ({
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
                      onChange={(value) => updateSelectedSubtitles((subtitle) => ({
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
                  onChange={(e) => updateSelectedSubtitles((subtitle) => ({
                    ...subtitle,
                    style: {
                      ...subtitle.style,
                      shadow: { ...(resolvedStyle.shadow ?? { enabled: true, color: '#000000', blur: 4, offsetX: 2, offsetY: 2, opacity: 0.7 }), enabled: e.target.checked },
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
                      onChange={(color) => updateSelectedSubtitles((subtitle) => ({
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
                      onChange={(value) => updateSelectedSubtitles((subtitle) => ({
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
                      onChange={(value) => updateSelectedSubtitles((subtitle) => ({
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
                      onChange={(value) => updateSelectedSubtitles((subtitle) => ({
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
                      onChange={(value) => updateSelectedSubtitles((subtitle) => ({
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
          </div>
        ) : null}
      </div>
    </PanelCard>
  );
}
