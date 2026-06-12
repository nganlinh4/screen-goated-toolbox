import { RotateCcw } from '@/components/ui/MaterialIcon';
import { PanelSelect } from '@/components/ui/PanelSelect';
import { Slider } from '@/components/ui/Slider';
import { Checkbox } from '@/components/ui/checkbox';
import { useSettings } from '@/hooks/useSettings';
import {
  DEFAULT_NARRATION_GROUP_TEXT_BUDGET,
  MAX_NARRATION_GROUP_TEXT_BUDGET,
  MIN_NARRATION_GROUP_TEXT_BUDGET,
  type SubtitleNarrationGroupPreview,
} from '@/hooks/useSubtitleNarration';
import type { SubtitleSource } from '@/lib/subtitleGenerationPlan';

type PanelOption = {
  value: string;
  label: string;
  disabled?: boolean;
};

type NarrationStatus = {
  completedItems?: number;
  error?: string | null;
  errors: Array<{ subtitleId: string; message: string }>;
  message?: string;
  state: string;
} | null;

interface S2sState {
  canGenerate: boolean;
  handleCancel: () => void;
  handleGenerate: () => void;
  isGenerating: boolean;
  status: { message?: string } | null;
}

interface SubtitleNarrationState {
  canGenerateNarration: boolean;
  handleCancelNarration: () => void;
  handleGenerateNarration: () => void;
  isGeneratingNarration: boolean;
  narrationGroupCount: number;
  narrationGroupPreview: SubtitleNarrationGroupPreview | null;
  narrationTargetCount: number;
}

interface NarrationModeSectionsProps {
  generateLabel: string;
  groupBudgetLabel: string;
  groupTextBudget: number;
  hasSubtitleRange: boolean;
  narration: SubtitleNarrationState;
  narrationMode: 'subtitles' | 's2s';
  onSourceChange: (value: SubtitleSource) => void;
  readUnsplitSubtitles: boolean;
  s2s: S2sState;
  s2sLanguageOptions: PanelOption[];
  s2sSourceOptions: PanelOption[];
  s2sTargetLanguage: string;
  selectedMethodSupported: boolean;
  selectedSource: SubtitleSource;
  selectedSourceTrackId: string;
  setGroupTextBudget: (value: number) => void;
  setIsGroupSliderDragging: (value: boolean) => void;
  setNarrationMode: (mode: 'subtitles' | 's2s') => void;
  setReadUnsplitSubtitles: (value: boolean) => void;
  setS2sTargetLanguage: (value: string) => void;
  setSelectedSourceTrackId: (value: string) => void;
  sourceTrackOptions: PanelOption[];
  status: NarrationStatus;
  statusMessage: string;
  subtitlesAvailable: boolean;
}

export function NarrationModeSections({
  generateLabel,
  groupBudgetLabel,
  groupTextBudget,
  hasSubtitleRange,
  narration,
  narrationMode,
  onSourceChange,
  readUnsplitSubtitles,
  s2s,
  s2sLanguageOptions,
  s2sSourceOptions,
  s2sTargetLanguage,
  selectedMethodSupported,
  selectedSource,
  selectedSourceTrackId,
  setGroupTextBudget,
  setIsGroupSliderDragging,
  setNarrationMode,
  setReadUnsplitSubtitles,
  setS2sTargetLanguage,
  setSelectedSourceTrackId,
  sourceTrackOptions,
  status,
  statusMessage,
  subtitlesAvailable,
}: NarrationModeSectionsProps) {
  const { t } = useSettings();
  const clampGroupBudget = (value: number) => setGroupTextBudget(
    Math.max(
      MIN_NARRATION_GROUP_TEXT_BUDGET,
      Math.min(MAX_NARRATION_GROUP_TEXT_BUDGET, Math.round(value)),
    ),
  );

  return (
    <>
      <div className="narration-mode-row grid grid-cols-2 gap-1 rounded-lg bg-surface-container-high/50 p-1">
        <button
          type="button"
          onClick={() => setNarrationMode('subtitles')}
          disabled={!subtitlesAvailable}
          className="narration-mode-subtitles ui-chip-button h-7 rounded-md text-[11px] font-medium disabled:opacity-45"
          data-active={narrationMode === 'subtitles'}
        >
          {t.narrationModeSubtitles}
        </button>
        <button
          type="button"
          onClick={() => setNarrationMode('s2s')}
          className="narration-mode-s2s ui-chip-button h-7 rounded-md text-[11px] font-medium"
          data-active={narrationMode === 's2s'}
        >
          {t.narrationModeS2s}
        </button>
      </div>

      {narrationMode === 's2s' && (
        <div className="narration-panel-s2s rounded-xl border border-outline/35 bg-surface-container-high/45 p-2.5">
          <div className="narration-s2s-title mb-2 text-[11px] font-semibold text-on-surface">
            {t.narrationModeS2s}
          </div>
          <div className="narration-s2s-source-row mb-2 flex items-center gap-2">
            <span className="w-20 flex-shrink-0 text-[11px] font-medium text-on-surface-variant">
              {t.subtitleSource}
            </span>
            <PanelSelect
              value={selectedSource}
              options={s2sSourceOptions}
              onChange={(value) => onSourceChange(value as SubtitleSource)}
              triggerClassName="narration-s2s-source-select h-8 flex-1 rounded-lg px-2.5 text-[11px]"
              contentClassName="narration-s2s-source-menu"
            />
          </div>
          <div className="narration-s2s-language-row mb-2 flex items-center gap-2">
            <span className="w-20 flex-shrink-0 text-[11px] font-medium text-on-surface-variant">
              {t.narrationS2sTarget}
            </span>
            <PanelSelect
              value={s2sTargetLanguage}
              options={s2sLanguageOptions}
              onChange={setS2sTargetLanguage}
              triggerClassName="narration-s2s-language-select h-8 flex-1 rounded-lg px-2.5 text-[11px]"
              contentClassName="narration-s2s-language-menu"
            />
          </div>
          <div className="narration-s2s-grouping mb-2 space-y-1.5">
            <div className="narration-s2s-grouping-header flex items-center justify-between gap-2">
              <span className="text-[11px] font-medium text-on-surface-variant">
                {t.narrationGrouping}
              </span>
              <span className="narration-s2s-grouping-value text-[10px] font-semibold text-on-surface-variant">
                {groupBudgetLabel}
              </span>
            </div>
            <Slider
              min={MIN_NARRATION_GROUP_TEXT_BUDGET}
              max={MAX_NARRATION_GROUP_TEXT_BUDGET}
              step={5}
              value={groupTextBudget}
              onChange={clampGroupBudget}
              className="narration-s2s-grouping-slider"
              disabled={s2s.isGenerating}
            />
          </div>
          <div className="narration-s2s-actions grid grid-cols-2 gap-1.5">
            <button
              type="button"
              disabled={!s2s.canGenerate}
              onClick={s2s.handleGenerate}
              data-tone="primary"
              data-emphasis="strong"
              className="narration-s2s-generate-button ui-action-button flex h-8 items-center justify-center rounded-lg px-2.5 text-[11px] font-medium"
            >
              {hasSubtitleRange ? t.subtitleGenerateForRange : t.subtitleNarrationGenerate}
            </button>
            <button
              type="button"
              disabled={!s2s.isGenerating}
              onClick={s2s.handleCancel}
              data-tone="danger"
              className="narration-s2s-cancel-button ui-action-button flex h-8 items-center justify-center rounded-lg px-2.5 text-[11px] font-medium"
            >
              {t.subtitleNarrationCancel}
            </button>
          </div>
          <div className="narration-s2s-status mt-2 truncate text-[10px] leading-4 text-on-surface-variant">
            {s2s.status?.message ?? t.subtitleNarrationIdleHint}
          </div>
        </div>
      )}

      {narrationMode === 'subtitles' && (
        <>
          <div className="narration-panel-source-row flex items-center gap-2">
            <span className="w-20 flex-shrink-0 text-[11px] font-medium text-on-surface-variant">
              {t.narrationSourceTrack}
            </span>
            <PanelSelect
              value={selectedSourceTrackId}
              options={sourceTrackOptions}
              onChange={(value) => setSelectedSourceTrackId(value)}
              triggerClassName="narration-source-select h-8 flex-1 rounded-lg px-2.5 text-[11px]"
              contentClassName="narration-source-menu"
            />
          </div>

          <div className="narration-panel-generate rounded-xl border border-outline/35 bg-surface-container-high/45 p-2.5">
            <div className="narration-panel-generate-header mb-2 flex items-center justify-between gap-2">
              <span className="text-[11px] font-semibold text-on-surface">
                {t.subtitleNarrationTitle}
              </span>
              <span className="text-[10px] font-medium text-on-surface-variant">
                {t.subtitleNarrationProgress
                  .replace('{done}', String(status?.completedItems ?? 0))
                  .replace('{total}', String(narration.narrationTargetCount))}
              </span>
            </div>
            <p className="narration-panel-hint mb-2 text-[10px] leading-4 text-on-surface-variant">
              {t.subtitleNarrationHint}
            </p>
            <label className="narration-panel-read-unsplit mb-2 flex cursor-pointer items-center gap-2 text-[11px] font-medium text-on-surface">
              <Checkbox
                checked={readUnsplitSubtitles}
                onChange={(event) => setReadUnsplitSubtitles(event.target.checked)}
              />
              {t.narrationReadUnsplitSubtitles}
            </label>
            <div className="narration-panel-grouping mb-2 space-y-1.5">
              <div className="narration-panel-grouping-header flex items-center justify-between gap-2">
                <span className="text-[11px] font-medium text-on-surface-variant">
                  {t.narrationGrouping}
                </span>
                <div className="narration-panel-grouping-meta flex items-center gap-1.5">
                  <span className="narration-panel-grouping-value text-[10px] font-semibold text-on-surface-variant">
                    {groupBudgetLabel} · {narration.narrationGroupCount} {t.narrationGroupingGroups}
                  </span>
                  {groupTextBudget !== DEFAULT_NARRATION_GROUP_TEXT_BUDGET ? (
                    <button
                      type="button"
                      onClick={() => setGroupTextBudget(DEFAULT_NARRATION_GROUP_TEXT_BUDGET)}
                      className="narration-panel-grouping-reset ui-chip-button flex h-6 w-6 items-center justify-center rounded-md"
                      title={t.subtitleGeminiPromptReset}
                      aria-label={t.subtitleGeminiPromptReset}
                    >
                      <RotateCcw className="h-3 w-3" />
                    </button>
                  ) : null}
                </div>
              </div>
              <Slider
                min={MIN_NARRATION_GROUP_TEXT_BUDGET}
                max={MAX_NARRATION_GROUP_TEXT_BUDGET}
                step={5}
                value={groupTextBudget}
                onChange={clampGroupBudget}
                onPointerDown={() => setIsGroupSliderDragging(true)}
                onPointerUp={() => setIsGroupSliderDragging(false)}
                onPointerCancel={() => setIsGroupSliderDragging(false)}
                onBlur={() => setIsGroupSliderDragging(false)}
                className="narration-panel-grouping-slider"
                disabled={narration.narrationTargetCount <= 1}
              />
            </div>
            <div className="narration-panel-actions grid grid-cols-2 gap-1.5">
              <button
                type="button"
                disabled={!narration.canGenerateNarration || !selectedMethodSupported}
                onClick={narration.handleGenerateNarration}
                data-tone="primary"
                data-emphasis="strong"
                className="narration-panel-generate-button ui-action-button flex h-8 items-center justify-center rounded-lg px-2.5 text-[11px] font-medium leading-tight"
              >
                {generateLabel}
              </button>
              <button
                type="button"
                disabled={!narration.isGeneratingNarration}
                onClick={narration.handleCancelNarration}
                data-tone="danger"
                className="narration-panel-cancel-button ui-action-button flex h-8 items-center justify-center rounded-lg px-2.5 text-[11px] font-medium leading-tight"
              >
                {t.subtitleNarrationCancel}
              </button>
            </div>
            <div className="narration-panel-status mt-2 flex items-center justify-between gap-2 text-[10px] leading-4 text-on-surface-variant">
              <span className="narration-panel-status-message min-w-0 truncate">{statusMessage}</span>
              {(status?.errors.length ?? 0) > 0 && (
                <span className="narration-panel-error-count flex-shrink-0 text-[var(--tertiary-color)]">
                  {t.subtitleNarrationErrors.replace('{count}', String(status?.errors.length ?? 0))}
                </span>
              )}
            </div>
            {(status?.state === 'error' || (status?.errors.length ?? 0) > 0) && (
              <details className="narration-panel-error-details mt-2 text-[10px] leading-4">
                <summary className="cursor-pointer text-[var(--tertiary-color)]">
                  {t.narrationErrorDetails}
                </summary>
                <div className="narration-panel-error-detail-body mt-1 max-h-32 overflow-y-auto rounded border border-outline/30 bg-[var(--ui-surface-2)] p-1.5 font-mono text-[10px] text-on-surface-variant">
                  {status?.error && (
                    <div className="narration-panel-error-top break-words text-[var(--tertiary-color)]">
                      {status.error}
                    </div>
                  )}
                  {(status?.errors ?? []).map((entry, idx) => (
                    <div key={idx} className="narration-panel-error-line break-words">
                      <span className="text-[var(--secondary-color)]">{entry.subtitleId}</span>
                      : {entry.message}
                    </div>
                  ))}
                  <div className="narration-panel-error-hint mt-1 text-[9px] italic text-on-surface-variant/70">
                    {t.narrationErrorLogHint}
                  </div>
                </div>
              </details>
            )}
          </div>
        </>
      )}
    </>
  );
}
