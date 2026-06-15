import { PanelSelect } from '@/components/ui/PanelSelect';
import { Slider } from '@/components/ui/Slider';
import { SettingRow } from '@/components/layout/SettingRow';
import { LanguageConfigList } from './LanguageConfigList';
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
        <span className="w-20 shrink-0 text-[11px] font-medium text-on-surface-variant">
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
        <LanguageConfigList
          className="narration-panel-kokoro-voices mb-2"
          title={t.narrationTtsKokoroVoiceConfigs}
          items={kokoroVoiceConfigs}
          removeTitle={t.narrationTtsLanguageConditionRemove}
          availableLanguages={availableKokoroVoiceLanguages}
          addLabel={t.narrationTtsKokoroVoiceConfigAdd}
          addTriggerClassName="narration-kokoro-voice-add h-8 self-start rounded-lg px-2.5 text-[11px]"
          addContentClassName="narration-kokoro-voice-add-menu"
          rowClassName="narration-panel-kokoro-voice-config"
          onAdd={addKokoroVoiceConfig}
          onRemove={removeKokoroVoiceConfig}
          renderControl={(config, index) => {
            const target = kokoroVoiceLanguageForCondition(config.languageCode);
            const options = (target
              ? kokoroVoices.filter((voice) => voice.languageCode === target)
              : kokoroVoices
            ).map((voice) => ({ value: voice.id, label: `${voice.id} · ${voice.label}` }));
            return (
              <PanelSelect
                value={config.voiceId}
                options={options}
                onChange={(value) => updateKokoroVoiceConfig(index, { voiceId: value })}
                triggerClassName="narration-kokoro-voice-select h-8 flex-1 rounded-lg px-2.5 text-[11px]"
                contentClassName="narration-kokoro-voice-menu"
                searchable={options.length > 8}
              />
            );
          }}
        />
        <SettingRow
          label={t.narrationTtsSpeed}
          valueDisplay={`${settings.kokoroSpeed.toFixed(2)}x`}
          className="narration-kokoro-speed-row mb-2"
        >
          <Slider
            min={0.5}
            max={2}
            step={0.05}
            value={settings.kokoroSpeed}
            onChange={(value) => update('kokoroSpeed', value)}
            className="narration-kokoro-speed-slider"
          />
        </SettingRow>
        <SettingRow
          label={t.narrationTtsKokoroThreads}
          valueDisplay={settings.kokoroNumThreads}
          className="narration-kokoro-threads-row"
        >
          <Slider
            min={1}
            max={8}
            step={1}
            value={settings.kokoroNumThreads}
            onChange={(value) => update('kokoroNumThreads', Math.round(value))}
            className="narration-kokoro-threads-slider"
          />
        </SettingRow>
      </>
    );
  }

  if (effectiveTtsMethod === 'Supertonic') {
    return (
      <>
        <LanguageConfigList
          className="narration-panel-supertonic-voices mb-2"
          title="Voice per language"
          items={supertonicVoiceConfigs}
          removeTitle={t.narrationTtsLanguageConditionRemove}
          availableLanguages={availableSupertonicLanguages}
          addLabel="Add language"
          addTriggerClassName="narration-supertonic-voice-add h-8 self-start rounded-lg px-2.5 text-[11px]"
          addContentClassName="narration-supertonic-voice-add-menu"
          rowClassName="narration-panel-supertonic-voice-config"
          onAdd={addSupertonicVoiceConfig}
          onRemove={removeSupertonicVoiceConfig}
          renderControl={(config, index) => (
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
          )}
        />
        <SettingRow
          label={t.narrationTtsSpeed}
          valueDisplay={`${settings.supertonicSpeed.toFixed(2)}x`}
          className="narration-supertonic-speed-row mb-2"
        >
          <Slider
            min={0.5}
            max={2}
            step={0.05}
            value={settings.supertonicSpeed}
            onChange={(value) => update('supertonicSpeed', value)}
            className="narration-supertonic-speed-slider"
          />
        </SettingRow>
        <SettingRow
          label="Steps"
          valueDisplay={settings.supertonicNumSteps}
          className="narration-supertonic-steps-row mb-2"
        >
          <Slider
            min={1}
            max={20}
            step={1}
            value={settings.supertonicNumSteps}
            onChange={(value) => update('supertonicNumSteps', Math.round(value))}
            className="narration-supertonic-steps-slider"
          />
        </SettingRow>
        <SettingRow
          label="Threads"
          valueDisplay={settings.supertonicNumThreads}
          className="narration-supertonic-threads-row"
        >
          <Slider
            min={1}
            max={8}
            step={1}
            value={settings.supertonicNumThreads}
            onChange={(value) => update('supertonicNumThreads', Math.round(value))}
            className="narration-supertonic-threads-slider"
          />
        </SettingRow>
      </>
    );
  }

  if (effectiveTtsMethod === 'MagpieMultilingual') {
    return (
      <LanguageConfigList
        className="narration-panel-magpie-voices mb-2"
        title={t.narrationTtsKokoroVoiceConfigs}
        items={magpieVoiceConfigs}
        removeTitle={t.narrationTtsLanguageConditionRemove}
        availableLanguages={availableMagpieVoiceLanguages}
        addLabel={t.narrationTtsKokoroVoiceConfigAdd}
        addTriggerClassName="narration-magpie-voice-add h-8 self-start rounded-lg px-2.5 text-[11px]"
        addContentClassName="narration-magpie-voice-add-menu"
        rowClassName="narration-panel-magpie-voice-config"
        onAdd={addMagpieVoiceConfig}
        onRemove={removeMagpieVoiceConfig}
        renderControl={(config, index) => (
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
        )}
      />
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
        <SettingRow
          label={t.narrationTtsPitch}
          valueDisplay={settings.edgePitch}
          className="narration-edge-pitch-row mb-2"
        >
          <Slider
            min={-50}
            max={50}
            step={1}
            value={settings.edgePitch}
            onChange={(value) => update('edgePitch', Math.round(value))}
            className="narration-panel-pitch-slider"
          />
        </SettingRow>
        <SettingRow
          label={t.narrationTtsRate}
          valueDisplay={settings.edgeRate}
          className="narration-edge-rate-row"
        >
          <Slider
            min={-50}
            max={100}
            step={1}
            value={settings.edgeRate}
            onChange={(value) => update('edgeRate', Math.round(value))}
            className="narration-panel-rate-slider"
          />
        </SettingRow>
        <LanguageConfigList
          className="narration-panel-edge-voices mt-2"
          title={t.narrationTtsEdgeVoiceConfigs}
          items={edgeVoiceConfigs}
          removeTitle={t.narrationTtsLanguageConditionRemove}
          availableLanguages={availableEdgeVoiceLanguages}
          addLabel={t.narrationTtsEdgeVoiceConfigAdd}
          addTriggerClassName="narration-edge-voice-add h-8 self-start rounded-lg px-2.5 text-[11px]"
          addContentClassName="narration-edge-voice-add-menu"
          rowClassName="narration-panel-edge-voice-config"
          onAdd={addEdgeVoiceConfig}
          onRemove={removeEdgeVoiceConfig}
          statusContent={
            <>
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
            </>
          }
          renderControl={(config, index) => {
            const voiceOptions = edgeVoicesByLanguage[config.languageCode] ?? [];
            const options = voiceOptions.length > 0
              ? voiceOptions.map((voice) => ({
                  value: voice.shortName,
                  label: `${voice.shortName} · ${voice.gender}`,
                }))
              : [{ value: config.voiceName, label: config.voiceName }];
            return (
              <PanelSelect
                value={config.voiceName}
                options={options}
                onChange={(value) => updateEdgeVoiceConfig(index, { voiceName: value })}
                triggerClassName="narration-edge-voice-select h-8 flex-1 rounded-lg px-2.5 text-[11px]"
                contentClassName="narration-edge-voice-menu"
                searchable={voiceOptions.length > 8}
              />
            );
          }}
        />
      </>
    );
  }

  return null;
}
