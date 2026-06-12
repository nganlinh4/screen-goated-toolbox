import { X } from '@/components/ui/MaterialIcon';
import { PanelSelect } from '@/components/ui/PanelSelect';
import { useSettings } from '@/hooks/useSettings';
import {
  useNarrationSettings,
  type NarrationEdgeVoiceConfig,
  type NarrationKokoroVoiceConfig,
  type NarrationMagpieVoiceConfig,
  type NarrationSupertonicVoiceConfig,
  type NarrationTtsMethod,
} from '@/hooks/useNarrationSettings';
import { kokoroVoiceLanguageForCondition } from './narrationLanguageUtils';

type NarrationSettingsState = ReturnType<typeof useNarrationSettings>['settings'];
type NarrationSettingsUpdate = ReturnType<typeof useNarrationSettings>['update'];

type LanguageOption = { languageCode: string; languageName: string };
type VoiceOption = { id: string; label: string; languageCode?: string };
type EdgeVoiceOption = { shortName: string; gender: string };

interface NarrationVoiceProviderSettingsProps {
  availableEdgeVoiceLanguages: LanguageOption[];
  availableKokoroVoiceLanguages: LanguageOption[];
  availableMagpieVoiceLanguages: LanguageOption[];
  availableSupertonicLanguages: LanguageOption[];
  edgeVoiceConfigs: NarrationEdgeVoiceConfig[];
  edgeVoiceState?: 'idle' | 'loading' | 'loaded' | 'error';
  edgeVoicesByLanguage: Record<string, EdgeVoiceOption[]>;
  effectiveTtsMethod: NarrationTtsMethod;
  googleSpeedOptions: string[];
  kokoroVoiceConfigs: NarrationKokoroVoiceConfig[];
  kokoroVoices: VoiceOption[];
  magpieVoiceConfigs: NarrationMagpieVoiceConfig[];
  magpieVoices: VoiceOption[];
  referenceVoices: VoiceOption[];
  settings: NarrationSettingsState;
  stepAudioVoices: VoiceOption[];
  supertonicVoiceConfigs: NarrationSupertonicVoiceConfig[];
  supertonicVoices: VoiceOption[];
  update: NarrationSettingsUpdate;
  addEdgeVoiceConfig: (languageCode: string, languageName: string) => void;
  addKokoroVoiceConfig: (languageCode: string, languageName: string) => void;
  addMagpieVoiceConfig: (languageCode: string, languageName: string) => void;
  addSupertonicVoiceConfig: (languageCode: string, languageName: string) => void;
  removeEdgeVoiceConfig: (index: number) => void;
  removeKokoroVoiceConfig: (index: number) => void;
  removeMagpieVoiceConfig: (index: number) => void;
  removeSupertonicVoiceConfig: (index: number) => void;
  updateEdgeVoiceConfig: (
    index: number,
    next: Partial<NarrationEdgeVoiceConfig>,
  ) => void;
  updateKokoroVoiceConfig: (
    index: number,
    next: Partial<NarrationKokoroVoiceConfig>,
  ) => void;
  updateMagpieVoiceConfig: (
    index: number,
    next: Partial<NarrationMagpieVoiceConfig>,
  ) => void;
  updateSupertonicVoiceConfig: (
    index: number,
    next: Partial<NarrationSupertonicVoiceConfig>,
  ) => void;
}

export function NarrationVoiceProviderSettings({
  addEdgeVoiceConfig,
  addKokoroVoiceConfig,
  addMagpieVoiceConfig,
  addSupertonicVoiceConfig,
  availableEdgeVoiceLanguages,
  availableKokoroVoiceLanguages,
  availableMagpieVoiceLanguages,
  availableSupertonicLanguages,
  edgeVoiceConfigs,
  edgeVoiceState,
  edgeVoicesByLanguage,
  effectiveTtsMethod,
  googleSpeedOptions,
  kokoroVoiceConfigs,
  kokoroVoices,
  magpieVoiceConfigs,
  magpieVoices,
  referenceVoices,
  removeEdgeVoiceConfig,
  removeKokoroVoiceConfig,
  removeMagpieVoiceConfig,
  removeSupertonicVoiceConfig,
  settings,
  stepAudioVoices,
  supertonicVoiceConfigs,
  supertonicVoices,
  update,
  updateEdgeVoiceConfig,
  updateKokoroVoiceConfig,
  updateMagpieVoiceConfig,
  updateSupertonicVoiceConfig,
}: NarrationVoiceProviderSettingsProps) {
  const { t } = useSettings();

  if (effectiveTtsMethod === 'GoogleTranslate') {
    return (
      <div className="narration-panel-row flex items-center gap-2">
        <span className="w-20 flex-shrink-0 text-[11px] font-medium text-on-surface-variant">
          {t.narrationTtsSpeed}
        </span>
        <PanelSelect
          value={settings.googleSpeed}
          options={googleSpeedOptions.map((speed) => ({ value: speed, label: speed }))}
          onChange={(value) => update('googleSpeed', value)}
          triggerClassName="narration-google-speed h-8 flex-1 rounded-lg px-2.5 text-[11px]"
          contentClassName="narration-google-speed-menu"
        />
      </div>
    );
  }

  if (effectiveTtsMethod === 'Kokoro') {
    return (
      <>
        <div className="narration-panel-kokoro-voices mb-2 flex flex-col gap-1.5">
          <span className="text-[11px] font-medium text-on-surface-variant">
            {t.narrationTtsKokoroVoiceConfigs}
          </span>
          {kokoroVoiceConfigs.map((config, index) => {
            const target = kokoroVoiceLanguageForCondition(config.languageCode);
            const options = (target
              ? kokoroVoices.filter((voice) => voice.languageCode === target)
              : kokoroVoices
            ).map((voice) => ({ value: voice.id, label: `${voice.id} · ${voice.label}` }));
            return (
              <div key={`${config.languageCode}-${index}`} className="narration-panel-kokoro-voice-config flex items-center gap-1.5">
                <span className="w-20 flex-shrink-0 truncate text-[11px] font-medium text-[var(--secondary-color)]">
                  {config.languageName}
                </span>
                <PanelSelect
                  value={config.voiceId}
                  options={options}
                  onChange={(value) => updateKokoroVoiceConfig(index, { voiceId: value })}
                  triggerClassName="narration-kokoro-voice-select h-8 flex-1 rounded-lg px-2.5 text-[11px]"
                  contentClassName="narration-kokoro-voice-menu"
                  searchable={options.length > 8}
                />
                <button
                  type="button"
                  onClick={() => removeKokoroVoiceConfig(index)}
                  className="ui-icon-button h-6 w-6 rounded-full text-on-surface-variant hover:text-[var(--tertiary-color)]"
                  title={t.narrationTtsLanguageConditionRemove}
                >
                  <X className="h-3 w-3" />
                </button>
              </div>
            );
          })}
          {availableKokoroVoiceLanguages.length > 0 && (
            <PanelSelect
              value={t.narrationTtsKokoroVoiceConfigAdd}
              options={availableKokoroVoiceLanguages.map((language) => ({
                value: language.languageCode,
                label: language.languageName,
              }))}
              onChange={(value) => {
                const language = availableKokoroVoiceLanguages.find(
                  (item) => item.languageCode === value,
                );
                if (language) addKokoroVoiceConfig(language.languageCode, language.languageName);
              }}
              triggerClassName="narration-kokoro-voice-add h-8 self-start rounded-lg px-2.5 text-[11px]"
              contentClassName="narration-kokoro-voice-add-menu"
              searchable
            />
          )}
        </div>
        <div className="narration-panel-row mb-2 flex items-center gap-2">
          <span className="w-20 flex-shrink-0 text-[10px] font-medium text-on-surface-variant">
            {t.narrationTtsSpeed}
          </span>
          <input
            type="range"
            min={0.5}
            max={2}
            step={0.05}
            value={settings.kokoroSpeed}
            onChange={(event) => update('kokoroSpeed', parseFloat(event.target.value))}
            className="narration-kokoro-speed-slider flex-1"
          />
          <span className="w-12 text-right text-[10px] tabular-nums text-on-surface">
            {settings.kokoroSpeed.toFixed(2)}x
          </span>
        </div>
        <div className="narration-panel-row flex items-center gap-2">
          <span className="w-20 flex-shrink-0 text-[10px] font-medium text-on-surface-variant">
            {t.narrationTtsKokoroThreads}
          </span>
          <input
            type="range"
            min={1}
            max={8}
            step={1}
            value={settings.kokoroNumThreads}
            onChange={(event) => update('kokoroNumThreads', parseInt(event.target.value, 10))}
            className="narration-kokoro-threads-slider flex-1"
          />
          <span className="w-12 text-right text-[10px] tabular-nums text-on-surface">
            {settings.kokoroNumThreads}
          </span>
        </div>
      </>
    );
  }

  if (effectiveTtsMethod === 'Supertonic') {
    return (
      <>
        <div className="narration-panel-supertonic-voices mb-2 flex flex-col gap-1.5">
          <span className="text-[11px] font-medium text-on-surface-variant">
            Voice per language
          </span>
          {supertonicVoiceConfigs.map((config, index) => (
            <div key={`${config.languageCode}-${index}`} className="narration-panel-supertonic-voice-config flex items-center gap-1.5">
              <span className="w-20 flex-shrink-0 truncate text-[11px] font-medium text-[var(--secondary-color)]">
                {config.languageName}
              </span>
              <PanelSelect
                value={config.voiceId}
                options={supertonicVoices.map((voice) => ({
                  value: voice.id,
                  label: voice.label,
                }))}
                onChange={(value) => updateSupertonicVoiceConfig(index, { voiceId: value })}
                triggerClassName="narration-supertonic-voice-select h-8 flex-1 rounded-lg px-2.5 text-[11px]"
                contentClassName="narration-supertonic-voice-menu"
              />
              <button
                type="button"
                onClick={() => removeSupertonicVoiceConfig(index)}
                className="ui-icon-button h-6 w-6 rounded-full text-on-surface-variant hover:text-[var(--tertiary-color)]"
                title={t.narrationTtsLanguageConditionRemove}
              >
                <X className="h-3 w-3" />
              </button>
            </div>
          ))}
          {availableSupertonicLanguages.length > 0 && (
            <PanelSelect
              value="Add language"
              options={availableSupertonicLanguages.map((language) => ({
                value: language.languageCode,
                label: language.languageName,
              }))}
              onChange={(value) => {
                const language = availableSupertonicLanguages.find(
                  (item) => item.languageCode === value,
                );
                if (language) addSupertonicVoiceConfig(language.languageCode, language.languageName);
              }}
              triggerClassName="narration-supertonic-voice-add h-8 self-start rounded-lg px-2.5 text-[11px]"
              contentClassName="narration-supertonic-voice-add-menu"
              searchable
            />
          )}
        </div>
        <div className="narration-panel-row mb-2 flex items-center gap-2">
          <span className="w-20 flex-shrink-0 text-[10px] font-medium text-on-surface-variant">
            {t.narrationTtsSpeed}
          </span>
          <input
            type="range"
            min={0.5}
            max={2}
            step={0.05}
            value={settings.supertonicSpeed}
            onChange={(event) => update('supertonicSpeed', parseFloat(event.target.value))}
            className="narration-supertonic-speed-slider flex-1"
          />
          <span className="w-12 text-right text-[10px] tabular-nums text-on-surface">
            {settings.supertonicSpeed.toFixed(2)}x
          </span>
        </div>
        <div className="narration-panel-row mb-2 flex items-center gap-2">
          <span className="w-20 flex-shrink-0 text-[10px] font-medium text-on-surface-variant">
            Steps
          </span>
          <input
            type="range"
            min={1}
            max={20}
            step={1}
            value={settings.supertonicNumSteps}
            onChange={(event) => update('supertonicNumSteps', parseInt(event.target.value, 10))}
            className="narration-supertonic-steps-slider flex-1"
          />
          <span className="w-12 text-right text-[10px] tabular-nums text-on-surface">
            {settings.supertonicNumSteps}
          </span>
        </div>
        <div className="narration-panel-row flex items-center gap-2">
          <span className="w-20 flex-shrink-0 text-[10px] font-medium text-on-surface-variant">
            Threads
          </span>
          <input
            type="range"
            min={1}
            max={8}
            step={1}
            value={settings.supertonicNumThreads}
            onChange={(event) => update('supertonicNumThreads', parseInt(event.target.value, 10))}
            className="narration-supertonic-threads-slider flex-1"
          />
          <span className="w-12 text-right text-[10px] tabular-nums text-on-surface">
            {settings.supertonicNumThreads}
          </span>
        </div>
      </>
    );
  }

  if (effectiveTtsMethod === 'MagpieMultilingual') {
    return (
      <div className="narration-panel-magpie-voices mb-2 flex flex-col gap-1.5">
        <span className="text-[11px] font-medium text-on-surface-variant">
          {t.narrationTtsKokoroVoiceConfigs}
        </span>
        {magpieVoiceConfigs.map((config, index) => (
          <div key={`${config.languageCode}-${index}`} className="narration-panel-magpie-voice-config flex items-center gap-1.5">
            <span className="w-20 flex-shrink-0 truncate text-[11px] font-medium text-[var(--secondary-color)]">
              {config.languageName}
            </span>
            <PanelSelect
              value={config.voiceId}
              options={magpieVoices.map((voice) => ({
                value: voice.id,
                label: voice.label,
              }))}
              onChange={(value) => updateMagpieVoiceConfig(index, { voiceId: value })}
              triggerClassName="narration-magpie-voice-select h-8 flex-1 rounded-lg px-2.5 text-[11px]"
              contentClassName="narration-magpie-voice-menu"
            />
            <button
              type="button"
              onClick={() => removeMagpieVoiceConfig(index)}
              className="ui-icon-button h-6 w-6 rounded-full text-on-surface-variant hover:text-[var(--tertiary-color)]"
              title={t.narrationTtsLanguageConditionRemove}
            >
              <X className="h-3 w-3" />
            </button>
          </div>
        ))}
        {availableMagpieVoiceLanguages.length > 0 && (
          <PanelSelect
            value={t.narrationTtsKokoroVoiceConfigAdd}
            options={availableMagpieVoiceLanguages.map((language) => ({
              value: language.languageCode,
              label: language.languageName,
            }))}
            onChange={(value) => {
              const language = availableMagpieVoiceLanguages.find(
                (item) => item.languageCode === value,
              );
              if (language) addMagpieVoiceConfig(language.languageCode, language.languageName);
            }}
            triggerClassName="narration-magpie-voice-add h-8 self-start rounded-lg px-2.5 text-[11px]"
            contentClassName="narration-magpie-voice-add-menu"
            searchable
          />
        )}
      </div>
    );
  }

  if (effectiveTtsMethod === 'StepAudioEditX') {
    return (
      <div className="narration-panel-step-audio-reference mb-2 flex flex-col gap-1.5">
        <span className="text-[11px] font-medium text-on-surface-variant">
          Reference voice
        </span>
        <PanelSelect
          value={settings.stepAudioReferenceVoiceId}
          options={[
            { value: '', label: 'Bundled default reference' },
            ...stepAudioVoices.map((voice) => ({
              value: voice.id,
              label: voice.label || 'Untitled reference',
            })),
          ]}
          onChange={(value) => update('stepAudioReferenceVoiceId', value)}
          triggerClassName="narration-step-audio-reference-select h-8 rounded-lg px-2.5 text-[11px]"
          contentClassName="narration-step-audio-reference-menu"
          searchable
        />
      </div>
    );
  }

  if (effectiveTtsMethod === 'VieneuTts') {
    return (
      <div className="narration-panel-vieneu-reference mb-2 flex flex-col gap-1.5">
        <span className="text-[11px] font-medium text-on-surface-variant">
          Reference voice
        </span>
        <PanelSelect
          value={settings.vieneuReferenceVoiceId}
          options={[
            { value: '', label: 'Model default voice' },
            ...referenceVoices.map((voice) => ({
              value: voice.id,
              label: voice.label || 'Untitled reference',
            })),
          ]}
          onChange={(value) => update('vieneuReferenceVoiceId', value)}
          triggerClassName="narration-vieneu-reference-select h-8 rounded-lg px-2.5 text-[11px]"
          contentClassName="narration-vieneu-reference-menu"
          searchable
        />
      </div>
    );
  }

  if (effectiveTtsMethod === 'EdgeTTS') {
    return (
      <>
        <div className="narration-panel-row mb-2 flex items-center gap-2">
          <span className="w-20 flex-shrink-0 text-[10px] font-medium text-on-surface-variant">
            {t.narrationTtsPitch}
          </span>
          <input
            type="range"
            min={-50}
            max={50}
            step={1}
            value={settings.edgePitch}
            onChange={(event) => update('edgePitch', parseInt(event.target.value, 10))}
            className="narration-panel-pitch-slider flex-1"
          />
          <span className="w-12 text-right text-[10px] tabular-nums text-on-surface">
            {settings.edgePitch}
          </span>
        </div>
        <div className="narration-panel-row flex items-center gap-2">
          <span className="w-20 flex-shrink-0 text-[10px] font-medium text-on-surface-variant">
            {t.narrationTtsRate}
          </span>
          <input
            type="range"
            min={-50}
            max={100}
            step={1}
            value={settings.edgeRate}
            onChange={(event) => update('edgeRate', parseInt(event.target.value, 10))}
            className="narration-panel-rate-slider flex-1"
          />
          <span className="w-12 text-right text-[10px] tabular-nums text-on-surface">
            {settings.edgeRate}
          </span>
        </div>
        <div className="narration-panel-edge-voices mt-2 flex flex-col gap-1.5">
          <span className="text-[11px] font-medium text-on-surface-variant">
            {t.narrationTtsEdgeVoiceConfigs}
          </span>
          {edgeVoiceState === 'loading' && (
            <span className="narration-panel-edge-loading text-[10px] text-on-surface-variant">
              {t.narrationTtsEdgeVoicesLoading}
            </span>
          )}
          {edgeVoiceState === 'error' && (
            <span className="narration-panel-edge-error text-[10px] text-[var(--tertiary-color)]">
              {t.narrationTtsEdgeVoicesFailed}
            </span>
          )}
          {edgeVoiceConfigs.map((config, index) => {
            const voiceOptions = edgeVoicesByLanguage[config.languageCode] ?? [];
            const options = voiceOptions.length > 0
              ? voiceOptions.map((voice) => ({
                  value: voice.shortName,
                  label: `${voice.shortName} · ${voice.gender}`,
                }))
              : [{ value: config.voiceName, label: config.voiceName }];
            return (
              <div
                key={`${config.languageCode}-${index}`}
                className="narration-panel-edge-voice-config flex items-center gap-1.5"
              >
                <span className="w-20 flex-shrink-0 truncate text-[11px] font-medium text-[var(--secondary-color)]">
                  {config.languageName}
                </span>
                <PanelSelect
                  value={config.voiceName}
                  options={options}
                  onChange={(value) => updateEdgeVoiceConfig(index, { voiceName: value })}
                  triggerClassName="narration-edge-voice-select h-8 flex-1 rounded-lg px-2.5 text-[11px]"
                  contentClassName="narration-edge-voice-menu"
                  searchable={voiceOptions.length > 8}
                />
                <button
                  type="button"
                  onClick={() => removeEdgeVoiceConfig(index)}
                  className="ui-icon-button h-6 w-6 rounded-full text-on-surface-variant hover:text-[var(--tertiary-color)]"
                  title={t.narrationTtsLanguageConditionRemove}
                >
                  <X className="h-3 w-3" />
                </button>
              </div>
            );
          })}
          {availableEdgeVoiceLanguages.length > 0 && (
            <PanelSelect
              value={t.narrationTtsEdgeVoiceConfigAdd}
              options={availableEdgeVoiceLanguages.map((language) => ({
                value: language.languageCode,
                label: language.languageName,
              }))}
              onChange={(value) => {
                const language = availableEdgeVoiceLanguages.find(
                  (item) => item.languageCode === value,
                );
                if (language) addEdgeVoiceConfig(language.languageCode, language.languageName);
              }}
              triggerClassName="narration-edge-voice-add h-8 self-start rounded-lg px-2.5 text-[11px]"
              contentClassName="narration-edge-voice-add-menu"
              searchable
            />
          )}
        </div>
      </>
    );
  }

  return null;
}
