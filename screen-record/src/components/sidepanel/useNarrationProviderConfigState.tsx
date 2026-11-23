import { useMemo } from 'react';
import { AlertTriangle } from '@/components/ui/MaterialIcon';
import { useSettings } from '@/hooks/useSettings';
import {
  useNarrationSettings,
  type NarrationEdgeVoiceConfig,
  type NarrationLanguageCondition,
  type NarrationTtsMethod,
} from '@/hooks/useNarrationSettings';
import { languageMatches } from './narrationLanguageUtils';

type NarrationSettingsState = ReturnType<typeof useNarrationSettings>['settings'];
type NarrationSettingsUpdate = ReturnType<typeof useNarrationSettings>['update'];
type NarrationMetadata = ReturnType<typeof useNarrationSettings>['metadata'];

interface ProviderConfigStateOptions {
  detectedNarrationLanguageCode: string | null;
  metadata: NarrationMetadata;
  settings: NarrationSettingsState;
  update: NarrationSettingsUpdate;
}

export function useNarrationProviderConfigState({
  detectedNarrationLanguageCode,
  metadata,
  settings,
  update,
}: ProviderConfigStateOptions) {
  const { t } = useSettings();
  const geminiVoices = metadata?.geminiVoices ?? [];
  const geminiModels = metadata?.geminiModels ?? [];
  const geminiInstructionLanguages = metadata?.geminiInstructionLanguages ?? [];
  const geminiSpeedOptions = metadata?.geminiSpeedOptions ?? ['Slow', 'Normal', 'Fast'];
  const googleSpeedOptions = metadata?.googleSpeedOptions ?? ['Slow', 'Normal'];
  const edgeVoiceLanguages = metadata?.edgeVoiceLanguages ?? [];
  const edgeVoicesByLanguage = metadata?.edgeVoicesByLanguage ?? {};
  const geminiLanguageConditions = settings.geminiLanguageConditions ?? [];
  const edgeVoiceConfigs = settings.edgeVoiceConfigs ?? [];

  const methodLabel = (method: NarrationTtsMethod, fallback: string) => {
    switch (method) {
      case 'GeminiLive':
        return t.narrationTtsMethodGemini;
      case 'EdgeTTS':
        return t.narrationTtsMethodEdge;
      case 'GoogleTranslate':
        return t.narrationTtsMethodGoogle;
      default:
        return fallback;
    }
  };

  const isMethodSupportedForDetectedLanguage = (method: NarrationTtsMethod) => {
    const detectedCode = detectedNarrationLanguageCode;
    if (!detectedCode) return true;
    switch (method) {
      case 'EdgeTTS':
        return edgeVoiceLanguages.length === 0
          || edgeVoiceLanguages.some((language) =>
            languageMatches(language.languageCode, detectedCode),
          );
      case 'GeminiLive':
        return geminiInstructionLanguages.length === 0
          || geminiInstructionLanguages.some((language) =>
            languageMatches(language.languageCode, detectedCode),
          );
      case 'GoogleTranslate':
      default:
        return true;
    }
  };

  const detectedLanguageLabel = useMemo(() => {
    const detectedCode = detectedNarrationLanguageCode;
    if (!detectedCode) return null;
    const allLanguages = [
      ...geminiInstructionLanguages,
      ...edgeVoiceLanguages,
    ];
    const match = allLanguages.find((language) =>
      languageMatches(language.languageCode, detectedCode),
    );
    return match?.languageName ?? detectedCode;
  }, [
    detectedNarrationLanguageCode,
    edgeVoiceLanguages,
    geminiInstructionLanguages,
  ]);

  const unsupportedLanguageTitle = detectedLanguageLabel
    ? t.narrationTtsUnsupportedForLanguage.replace('{language}', detectedLanguageLabel)
    : t.narrationTtsUnsupported;
  const providerOptions = (metadata?.providers?.length
    ? metadata.providers
    : [
        { method: 'GeminiLive' as const, label: 'Gemini Live' },
        { method: 'EdgeTTS' as const, label: 'Edge TTS' },
        { method: 'GoogleTranslate' as const, label: 'Google Translate' },
      ]).map((provider) => {
        const isSupported = isMethodSupportedForDetectedLanguage(provider.method);
        return {
          value: provider.method,
          label: methodLabel(provider.method, provider.label),
          disabled: !isSupported,
          trailing: !isSupported ? (
            <span className="narration-method-language-warning-wrapper" title={unsupportedLanguageTitle}>
              <AlertTriangle
                className="narration-method-language-warning h-3.5 w-3.5 text-[var(--tertiary-color)]"
                aria-label={unsupportedLanguageTitle}
              />
            </span>
          ) : undefined,
        };
      });

  const usedConditionCodes = new Set(
    geminiLanguageConditions.map((condition) => condition.languageCode.toLowerCase()),
  );
  const availableConditionLanguages = geminiInstructionLanguages.filter(
    (language) => !usedConditionCodes.has(language.languageCode.toLowerCase()),
  );
  const updateLanguageCondition = (
    index: number,
    next: Partial<NarrationLanguageCondition>,
  ) => {
    update('geminiLanguageConditions', geminiLanguageConditions.map((condition, i) =>
      i === index ? { ...condition, ...next } : condition,
    ));
  };
  const removeLanguageCondition = (index: number) => {
    update('geminiLanguageConditions', geminiLanguageConditions.filter((_, i) => i !== index));
  };
  const addLanguageCondition = (languageCode: string, languageName: string) => {
    update('geminiLanguageConditions', [
      ...geminiLanguageConditions,
      { languageCode, languageName, instruction: '' },
    ]);
  };

  const setEdgeVoiceConfigs = (configs: NarrationEdgeVoiceConfig[]) => {
    update('edgeVoiceConfigs', configs);
    update('edgeVoice', configs[0]?.voiceName ?? settings.edgeVoice);
  };
  const updateEdgeVoiceConfig = (index: number, next: Partial<NarrationEdgeVoiceConfig>) => {
    setEdgeVoiceConfigs(edgeVoiceConfigs.map((config, i) =>
      i === index ? { ...config, ...next } : config,
    ));
  };
  const removeEdgeVoiceConfig = (index: number) => {
    setEdgeVoiceConfigs(edgeVoiceConfigs.filter((_, i) => i !== index));
  };
  const addEdgeVoiceConfig = (languageCode: string, languageName: string) => {
    const voices = edgeVoicesByLanguage[languageCode] ?? [];
    const voiceName = voices[0]?.shortName ?? `${languageCode}-??-??Neural`;
    setEdgeVoiceConfigs([...edgeVoiceConfigs, { languageCode, languageName, voiceName }]);
  };
  const usedEdgeVoiceCodes = new Set(
    edgeVoiceConfigs.map((config) => config.languageCode.toLowerCase()),
  );
  const availableEdgeVoiceLanguages = edgeVoiceLanguages.filter(
    (language) => !usedEdgeVoiceCodes.has(language.languageCode.toLowerCase()),
  );

  return {
    addEdgeVoiceConfig,
    addLanguageCondition,
    availableConditionLanguages,
    availableEdgeVoiceLanguages,
    detectedLanguageLabel,
    edgeVoiceConfigs,
    edgeVoiceLanguages,
    edgeVoicesByLanguage,
    geminiInstructionLanguages,
    geminiLanguageConditions,
    geminiModels,
    geminiSpeedOptions,
    geminiVoices,
    googleSpeedOptions,
    isMethodSupportedForDetectedLanguage,
    providerOptions,
    removeEdgeVoiceConfig,
    removeLanguageCondition,
    updateEdgeVoiceConfig,
    updateLanguageCondition,
  };
}
