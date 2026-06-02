import { X } from 'lucide-react';
import { PanelSelect } from '@/components/ui/PanelSelect';
import { Slider } from '@/components/ui/Slider';
import { useSettings } from '@/hooks/useSettings';
import {
  useNarrationSettings,
  type NarrationLanguageCondition,
} from '@/hooks/useNarrationSettings';

type NarrationSettingsState = ReturnType<typeof useNarrationSettings>['settings'];
type NarrationSettingsUpdate = ReturnType<typeof useNarrationSettings>['update'];

interface NarrationGeminiSettingsProps {
  availableConditionLanguages: Array<{ languageCode: string; languageName: string }>;
  geminiLanguageConditions: NarrationLanguageCondition[];
  geminiModels: Array<{ apiModel: string; label: string }>;
  geminiSpeedOptions: string[];
  geminiVoices: Array<{ name: string; gender: string }>;
  narrationMode: 'subtitles' | 's2s';
  settings: NarrationSettingsState;
  update: NarrationSettingsUpdate;
  addLanguageCondition: (languageCode: string, languageName: string) => void;
  removeLanguageCondition: (index: number) => void;
  updateLanguageCondition: (
    index: number,
    next: Partial<NarrationLanguageCondition>,
  ) => void;
}

export function NarrationGeminiSettings({
  addLanguageCondition,
  availableConditionLanguages,
  geminiLanguageConditions,
  geminiModels,
  geminiSpeedOptions,
  geminiVoices,
  narrationMode,
  removeLanguageCondition,
  settings,
  update,
  updateLanguageCondition,
}: NarrationGeminiSettingsProps) {
  const { t } = useSettings();

  return (
    <>
      {geminiModels.length > 0 && (
        <div className="narration-panel-row mb-2 flex items-center gap-2">
          <span className="w-20 flex-shrink-0 text-[11px] font-medium text-on-surface-variant">
            {t.narrationTtsModel}
          </span>
          <PanelSelect
            value={settings.geminiModel}
            options={geminiModels.map((model) => ({
              value: model.apiModel,
              label: model.label,
            }))}
            onChange={(value) => update('geminiModel', value)}
            triggerClassName="narration-model-select h-8 flex-1 rounded-lg px-2.5 text-[11px]"
            contentClassName="narration-model-menu"
          />
        </div>
      )}
      <div className="narration-panel-row mb-2 flex items-center gap-2">
        <span className="w-20 flex-shrink-0 text-[11px] font-medium text-on-surface-variant">
          {t.narrationTtsVoice}
        </span>
        <PanelSelect
          value={settings.geminiVoice}
          options={geminiVoices.map((voice) => ({
            value: voice.name,
            label: `${voice.name} · ${voice.gender}`,
          }))}
          onChange={(value) => update('geminiVoice', value)}
          triggerClassName="narration-voice-select h-8 flex-1 rounded-lg px-2.5 text-[11px]"
          contentClassName="narration-voice-menu"
          searchable
        />
      </div>
      <div className="narration-panel-row mb-2 flex items-center gap-2">
        <span className="w-20 flex-shrink-0 text-[11px] font-medium text-on-surface-variant">
          {t.narrationTtsSpeed}
        </span>
        <PanelSelect
          value={settings.geminiSpeed}
          options={geminiSpeedOptions.map((speed) => ({
            value: speed,
            label: speed,
          }))}
          onChange={(value) => update('geminiSpeed', value)}
          triggerClassName="narration-speed-select h-8 flex-1 rounded-lg px-2.5 text-[11px]"
          contentClassName="narration-speed-menu"
        />
      </div>
      <div className="narration-panel-parallel mb-2 space-y-1.5">
        <div className="narration-panel-parallel-header flex items-center justify-between gap-2">
          <span className="text-[11px] font-medium text-on-surface-variant">
            {t.narrationTtsParallelRequests}
          </span>
          <span className="narration-panel-parallel-value text-[10px] font-semibold text-on-surface-variant">
            {narrationMode === 's2s'
              ? settings.geminiS2sParallelRequests
              : settings.geminiParallelRequests}
          </span>
        </div>
        <Slider
          min={1}
          max={narrationMode === 's2s' ? 6 : 4}
          step={1}
          value={narrationMode === 's2s'
            ? settings.geminiS2sParallelRequests
            : settings.geminiParallelRequests}
          onChange={(value) => {
            const next = Math.round(value);
            if (narrationMode === 's2s') {
              update('geminiS2sParallelRequests', Math.max(1, Math.min(6, next)));
            } else {
              update('geminiParallelRequests', Math.max(1, Math.min(4, next)));
            }
          }}
          className="narration-panel-parallel-slider"
        />
        <div className="narration-panel-parallel-warning text-[9px] leading-3 text-on-surface-variant/75">
          {t.narrationTtsParallelRequestsWarning}
        </div>
      </div>
      <div className="narration-panel-row mb-2 flex flex-col gap-1">
        <span className="text-[11px] font-medium text-on-surface-variant">
          {t.narrationTtsInstruction}
        </span>
        <textarea
          value={settings.geminiInstruction}
          onChange={(event) => update('geminiInstruction', event.target.value)}
          rows={2}
          className="narration-panel-instruction ui-input w-full resize-y rounded-lg px-2 py-1 text-[11px]"
        />
      </div>
      <div className="narration-panel-conditions mb-1 flex flex-col gap-1.5">
        <span className="text-[11px] font-medium text-on-surface-variant">
          {t.narrationTtsLanguageConditions}
        </span>
        {geminiLanguageConditions.map((condition, index) => (
          <div key={`${condition.languageCode}-${index}`} className="narration-panel-condition flex items-center gap-1.5">
            <span className="w-20 flex-shrink-0 text-[11px] font-medium text-[var(--secondary-color)]">
              {condition.languageName}
            </span>
            <input
              value={condition.instruction}
              onChange={(event) =>
                updateLanguageCondition(index, { instruction: event.target.value })
              }
              placeholder={t.narrationTtsLanguageConditionHint}
              className="narration-panel-condition-input ui-input flex-1 rounded-lg px-2 py-1 text-[11px]"
            />
            <button
              type="button"
              onClick={() => removeLanguageCondition(index)}
              className="ui-icon-button h-6 w-6 rounded-full text-on-surface-variant hover:text-[var(--tertiary-color)]"
              title={t.narrationTtsLanguageConditionRemove}
            >
              <X className="h-3 w-3" />
            </button>
          </div>
        ))}
        {availableConditionLanguages.length > 0 && (
          <PanelSelect
            value={t.narrationTtsLanguageConditionAdd}
            options={availableConditionLanguages.map((lang) => ({
              value: lang.languageCode,
              label: lang.languageName,
            }))}
            onChange={(value) => {
              if (!value) return;
              const lang = availableConditionLanguages.find((item) => item.languageCode === value);
              if (lang) addLanguageCondition(lang.languageCode, lang.languageName);
            }}
            triggerClassName="narration-condition-add h-8 self-start rounded-lg px-2.5 text-[11px]"
            contentClassName="narration-condition-add-menu"
            searchable
          />
        )}
      </div>
    </>
  );
}
