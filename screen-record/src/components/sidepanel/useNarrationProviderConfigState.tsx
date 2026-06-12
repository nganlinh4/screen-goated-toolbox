import { useMemo } from 'react';
import { AlertTriangle } from '@/components/ui/MaterialIcon';
import { useSettings } from '@/hooks/useSettings';
import {
  useNarrationSettings,
  type NarrationEdgeVoiceConfig,
  type NarrationKokoroVoiceConfig,
  type NarrationLanguageCondition,
  type NarrationMagpieVoiceConfig,
  type NarrationSupertonicVoiceConfig,
  type NarrationTtsMethod,
} from '@/hooks/useNarrationSettings';
import {
  kokoroVoiceLanguageForCondition,
  languageMatches,
} from './narrationLanguageUtils';

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
  const kokoroVoices = metadata?.kokoroVoices ?? [];
  const kokoroVoiceLanguages = metadata?.kokoroVoiceLanguages ?? [];
  const magpieVoices = metadata?.magpieVoices ?? [];
  const magpieVoiceLanguages = metadata?.magpieVoiceLanguages ?? [];
  const supertonicLanguages = metadata?.supertonicLanguages ?? [];
  const supertonicVoices = metadata?.supertonicVoices ?? [];
  const stepAudioVoices = metadata?.stepAudioVoices ?? [];
  const stepAudioVoiceLanguages = metadata?.stepAudioVoiceLanguages ?? [];
  const referenceVoices = metadata?.stepAudioReferenceVoices ?? stepAudioVoices;
  const edgeVoiceLanguages = metadata?.edgeVoiceLanguages ?? [];
  const edgeVoicesByLanguage = metadata?.edgeVoicesByLanguage ?? {};
  const geminiLanguageConditions = settings.geminiLanguageConditions ?? [];
  const edgeVoiceConfigs = settings.edgeVoiceConfigs ?? [];
  const kokoroVoiceConfigs = settings.kokoroVoiceConfigs ?? [];
  const magpieVoiceConfigs = settings.magpieVoiceConfigs ?? [];
  const supertonicVoiceConfigs = settings.supertonicVoiceConfigs ?? [];

  const methodLabel = (method: NarrationTtsMethod, fallback: string) => {
    switch (method) {
      case 'GeminiLive':
        return t.narrationTtsMethodGemini;
      case 'EdgeTTS':
        return t.narrationTtsMethodEdge;
      case 'GoogleTranslate':
        return t.narrationTtsMethodGoogle;
      case 'Kokoro':
        return t.narrationTtsMethodKokoro;
      case 'Supertonic':
        return 'Supertonic 3';
      case 'VieneuTts':
        return 'VieNeu-TTS v2';
      case 'StepAudioEditX':
        return 'Step Audio EditX';
      case 'MagpieMultilingual':
        return 'NVIDIA Magpie-Multilingual 357M';
      default:
        return fallback;
    }
  };

  const isMethodSupportedForDetectedLanguage = (method: NarrationTtsMethod) => {
    const detectedCode = detectedNarrationLanguageCode;
    if (!detectedCode) return true;
    switch (method) {
      case 'Kokoro':
        return kokoroVoiceLanguages.length === 0
          || kokoroVoiceLanguages.some((language) =>
            languageMatches(language.languageCode, detectedCode),
          );
      case 'Supertonic':
        return supertonicLanguages.length === 0
          || supertonicLanguages.some((language) =>
            languageMatches(language.languageCode, detectedCode),
          );
      case 'MagpieMultilingual':
        return magpieVoiceLanguages.length === 0
          || magpieVoiceLanguages.some((language) =>
            languageMatches(language.languageCode, detectedCode),
          );
      case 'VieneuTts':
        return languageMatches('vie', detectedCode);
      case 'StepAudioEditX':
        return stepAudioVoiceLanguages.length === 0
          || stepAudioVoiceLanguages.some((language) =>
            languageMatches(language.languageCode, detectedCode),
          );
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
      ...kokoroVoiceLanguages,
      ...magpieVoiceLanguages,
      ...supertonicLanguages,
      ...stepAudioVoiceLanguages,
    ];
    const match = allLanguages.find((language) =>
      languageMatches(language.languageCode, detectedCode),
    );
    return match?.languageName ?? detectedCode;
  }, [
    detectedNarrationLanguageCode,
    edgeVoiceLanguages,
    geminiInstructionLanguages,
    kokoroVoiceLanguages,
    magpieVoiceLanguages,
    stepAudioVoiceLanguages,
    supertonicLanguages,
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
        { method: 'Kokoro' as const, label: 'Kokoro 82M v1.0' },
        { method: 'Supertonic' as const, label: 'Supertonic 3' },
        { method: 'VieneuTts' as const, label: 'VieNeu-TTS v2' },
        { method: 'StepAudioEditX' as const, label: 'Step Audio EditX' },
        { method: 'MagpieMultilingual' as const, label: 'NVIDIA Magpie-Multilingual 357M' },
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

  const setKokoroVoiceConfigs = (configs: NarrationKokoroVoiceConfig[]) => {
    update('kokoroVoiceConfigs', configs);
    update('kokoroVoice', configs[0]?.voiceId ?? settings.kokoroVoice);
  };
  const updateKokoroVoiceConfig = (index: number, next: Partial<NarrationKokoroVoiceConfig>) => {
    setKokoroVoiceConfigs(kokoroVoiceConfigs.map((config, i) =>
      i === index ? { ...config, ...next } : config,
    ));
  };
  const removeKokoroVoiceConfig = (index: number) => {
    setKokoroVoiceConfigs(kokoroVoiceConfigs.filter((_, i) => i !== index));
  };
  const addKokoroVoiceConfig = (languageCode: string, languageName: string) => {
    const normalized = kokoroVoiceLanguageForCondition(languageCode);
    const voiceId = kokoroVoices.find((voice) => voice.languageCode === normalized)?.id
      ?? kokoroVoices[0]?.id
      ?? 'af_heart';
    setKokoroVoiceConfigs([...kokoroVoiceConfigs, { languageCode, languageName, voiceId }]);
  };
  const usedKokoroVoiceCodes = new Set(
    kokoroVoiceConfigs.map((config) => config.languageCode.toLowerCase()),
  );
  const availableKokoroVoiceLanguages = kokoroVoiceLanguages.filter(
    (language) => !usedKokoroVoiceCodes.has(language.languageCode.toLowerCase()),
  );

  const setMagpieVoiceConfigs = (configs: NarrationMagpieVoiceConfig[]) => {
    update('magpieVoiceConfigs', configs);
  };
  const updateMagpieVoiceConfig = (index: number, next: Partial<NarrationMagpieVoiceConfig>) => {
    setMagpieVoiceConfigs(magpieVoiceConfigs.map((config, i) =>
      i === index ? { ...config, ...next } : config,
    ));
  };
  const removeMagpieVoiceConfig = (index: number) => {
    setMagpieVoiceConfigs(magpieVoiceConfigs.filter((_, i) => i !== index));
  };
  const addMagpieVoiceConfig = (languageCode: string, languageName: string) => {
    setMagpieVoiceConfigs([
      ...magpieVoiceConfigs,
      { languageCode, languageName, voiceId: magpieVoices[0]?.id ?? 'Sofia' },
    ]);
  };
  const usedMagpieVoiceCodes = new Set(
    magpieVoiceConfigs.map((config) => config.languageCode.toLowerCase()),
  );
  const availableMagpieVoiceLanguages = magpieVoiceLanguages.filter(
    (language) => !usedMagpieVoiceCodes.has(language.languageCode.toLowerCase()),
  );

  const setSupertonicVoiceConfigs = (configs: NarrationSupertonicVoiceConfig[]) => {
    update('supertonicVoiceConfigs', configs);
  };
  const updateSupertonicVoiceConfig = (
    index: number,
    next: Partial<NarrationSupertonicVoiceConfig>,
  ) => {
    setSupertonicVoiceConfigs(supertonicVoiceConfigs.map((config, i) =>
      i === index ? { ...config, ...next } : config,
    ));
  };
  const removeSupertonicVoiceConfig = (index: number) => {
    setSupertonicVoiceConfigs(supertonicVoiceConfigs.filter((_, i) => i !== index));
  };
  const addSupertonicVoiceConfig = (languageCode: string, languageName: string) => {
    setSupertonicVoiceConfigs([
      ...supertonicVoiceConfigs,
      { languageCode, languageName, voiceId: supertonicVoices[0]?.id ?? 'M1' },
    ]);
  };
  const usedSupertonicVoiceCodes = new Set(
    supertonicVoiceConfigs.map((config) => config.languageCode.toLowerCase()),
  );
  const availableSupertonicLanguages = supertonicLanguages.filter(
    (language) => !usedSupertonicVoiceCodes.has(language.languageCode.toLowerCase()),
  );

  return {
    addEdgeVoiceConfig,
    addKokoroVoiceConfig,
    addLanguageCondition,
    addMagpieVoiceConfig,
    addSupertonicVoiceConfig,
    availableConditionLanguages,
    availableEdgeVoiceLanguages,
    availableKokoroVoiceLanguages,
    availableMagpieVoiceLanguages,
    availableSupertonicLanguages,
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
    kokoroVoiceConfigs,
    kokoroVoiceLanguages,
    kokoroVoices,
    magpieVoiceConfigs,
    magpieVoiceLanguages,
    magpieVoices,
    providerOptions,
    referenceVoices,
    removeEdgeVoiceConfig,
    removeKokoroVoiceConfig,
    removeLanguageCondition,
    removeMagpieVoiceConfig,
    removeSupertonicVoiceConfig,
    stepAudioVoiceLanguages,
    stepAudioVoices,
    supertonicLanguages,
    supertonicVoiceConfigs,
    supertonicVoices,
    updateEdgeVoiceConfig,
    updateKokoroVoiceConfig,
    updateLanguageCondition,
    updateMagpieVoiceConfig,
    updateSupertonicVoiceConfig,
  };
}
