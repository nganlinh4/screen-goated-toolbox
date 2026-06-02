import { RotateCcw } from 'lucide-react';
import { PanelSelect } from '@/components/ui/PanelSelect';
import { Slider } from '@/components/ui/Slider';
import { Checkbox } from '@/components/ui/checkbox';
import type { useSubtitleTranslation } from '@/hooks/useSubtitleTranslation';
import type { Translations } from '@/i18n';
import { SubtitleCustomChainEditor } from './SubtitleCustomChainEditor';

interface SubtitleTranslationSettingsProps {
  t: Translations;
  subtitleTranslation: ReturnType<typeof useSubtitleTranslation>;
}

export function SubtitleTranslationSettings({
  t,
  subtitleTranslation,
}: SubtitleTranslationSettingsProps) {
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
  const translationStatusText = subtitleTranslation.subtitleTranslationStatusMessage
    ?? subtitleTranslation.subtitleTranslationCapabilities?.reason
    ?? t.subtitleTranslationHint;

  return (
    <>
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

      <div className="subtitle-translation-model-row flex items-center gap-2">
        <span className="w-20 flex-shrink-0 text-[11px] font-medium text-on-surface-variant">
          {t.subtitleTranslationModel}
        </span>
        <PanelSelect
          value={subtitleTranslation.subtitleTranslationModelId}
          options={subtitleTranslation.subtitleTranslationModelOptions}
          onChange={subtitleTranslation.setSubtitleTranslationModelId}
          triggerClassName="subtitle-translation-model-select h-8 flex-1 rounded-lg px-2.5 text-[11px]"
          contentClassName="subtitle-translation-model-menu"
        />
      </div>

      <label className="subtitle-translation-smart-fallback-row flex cursor-pointer items-center gap-2 text-[11px] font-medium text-on-surface">
        <Checkbox
          checked={subtitleTranslation.subtitleTranslationSmartFallback}
          onChange={(event) => subtitleTranslation.setSubtitleTranslationSmartFallback(event.target.checked)}
        />
        {t.subtitleTranslationSmartFallback}
      </label>

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
          <div className="subtitle-translation-chunk-meta flex items-center gap-1.5">
            <span className="subtitle-translation-chunk-value text-[10px] font-semibold text-on-surface-variant">
              {subtitleTranslation.subtitleTranslationChunkCountIsAuto
                ? `${t.subtitleTranslationChunkAuto}: `
                : ''}
              {subtitleTranslation.subtitleTranslationChunkCount}/{subtitleTranslation.subtitleTranslationChunkMax}
            </span>
            {!subtitleTranslation.subtitleTranslationChunkCountIsAuto ? (
              <button
                type="button"
                onClick={subtitleTranslation.resetSubtitleTranslationChunkCount}
                className="subtitle-translation-chunk-reset ui-chip-button flex h-6 w-6 items-center justify-center rounded-md"
                title={t.subtitleGeminiPromptReset}
                aria-label={t.subtitleGeminiPromptReset}
              >
                <RotateCcw className="h-3 w-3" />
              </button>
            ) : null}
          </div>
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
        <p className="subtitle-translation-instructions-hint text-[10px] leading-4 text-on-surface-variant">
          {t.subtitleTranslationInstructionsHint}
        </p>
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
          data-emphasis="strong"
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
    </>
  );
}
