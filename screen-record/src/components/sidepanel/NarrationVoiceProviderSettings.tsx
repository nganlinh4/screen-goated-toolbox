import { PanelSelect } from '@/components/ui/PanelSelect';
import { LabelledField } from '@/components/ui/FieldLabelContext';
import { Slider } from '@/components/ui/Slider';
import { SettingRow } from '@/components/layout/SettingRow';
import { LanguageConfigList } from './LanguageConfigList';
import { useSettings } from '@/hooks/useSettings';
import {
  useNarrationSettings,
  type NarrationEdgeVoiceConfig,
  type NarrationTtsMethod,
} from '@/hooks/useNarrationSettings';

type NarrationSettingsState = ReturnType<typeof useNarrationSettings>['settings'];
type NarrationSettingsUpdate = ReturnType<typeof useNarrationSettings>['update'];

type LanguageOption = { languageCode: string; languageName: string };
type EdgeVoiceOption = { shortName: string; gender: string };

interface NarrationVoiceProviderSettingsProps {
  availableEdgeVoiceLanguages: LanguageOption[];
  edgeVoiceConfigs: NarrationEdgeVoiceConfig[];
  edgeVoiceState?: 'idle' | 'loading' | 'loaded' | 'error';
  edgeVoicesByLanguage: Record<string, EdgeVoiceOption[]>;
  effectiveTtsMethod: NarrationTtsMethod;
  googleSpeedOptions: string[];
  settings: NarrationSettingsState;
  update: NarrationSettingsUpdate;
  addEdgeVoiceConfig: (languageCode: string, languageName: string) => void;
  removeEdgeVoiceConfig: (index: number) => void;
  updateEdgeVoiceConfig: (
    index: number,
    next: Partial<NarrationEdgeVoiceConfig>,
  ) => void;
}

export function NarrationVoiceProviderSettings({
  addEdgeVoiceConfig,
  availableEdgeVoiceLanguages,
  edgeVoiceConfigs,
  edgeVoiceState,
  edgeVoicesByLanguage,
  effectiveTtsMethod,
  googleSpeedOptions,
  removeEdgeVoiceConfig,
  settings,
  update,
  updateEdgeVoiceConfig,
}: NarrationVoiceProviderSettingsProps) {
  const { t } = useSettings();

  if (effectiveTtsMethod === 'GoogleTranslate') {
    return (
      <div className="narration-panel-row flex items-center gap-2">
        <LabelledField
          label={t.narrationTtsSpeed}
          labelClassName="w-20 shrink-0 text-[11px] font-medium text-on-surface-variant"
        >
          <PanelSelect
            value={settings.googleSpeed}
            options={googleSpeedOptions.map((speed) => ({ value: speed, label: speed }))}
            onChange={(value) => update('googleSpeed', value)}
            triggerClassName="narration-google-speed h-8 flex-1 rounded-lg px-2.5 text-[11px]"
            contentClassName="narration-google-speed-menu"
          />
        </LabelledField>
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
