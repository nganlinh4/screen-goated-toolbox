import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@/lib/ipc';

export type NarrationTtsMethod =
  | 'GeminiLive'
  | 'GoogleTranslate'
  | 'EdgeTTS'
  | 'Kokoro'
  | 'Supertonic'
  | 'StepAudioEditX'
  | 'MagpieMultilingual';

export interface NarrationLanguageCondition {
  languageCode: string;
  languageName: string;
  instruction: string;
}

export interface NarrationEdgeVoiceConfig {
  languageCode: string;
  languageName: string;
  voiceName: string;
}

export interface NarrationKokoroVoiceConfig {
  languageCode: string;
  languageName: string;
  voiceId: string;
}

export interface NarrationMagpieVoiceConfig {
  languageCode: string;
  languageName: string;
  voiceId: string;
}

export interface NarrationSupertonicVoiceConfig {
  languageCode: string;
  languageName: string;
  voiceId: string;
}

export interface NarrationStepAudioVoice {
  id: string;
  label: string;
}

export interface NarrationSettingsState {
  method: NarrationTtsMethod;
  geminiModel: string;
  geminiVoice: string;
  geminiSpeed: string;
  geminiInstruction: string;
  geminiLanguageConditions: NarrationLanguageCondition[];
  googleSpeed: string;
  edgeVoice: string;
  edgePitch: number;
  edgeRate: number;
  edgeVoiceConfigs: NarrationEdgeVoiceConfig[];
  kokoroVoice: string;
  kokoroSpeed: number;
  kokoroNumThreads: number;
  kokoroVoiceConfigs: NarrationKokoroVoiceConfig[];
  stepAudioVoice: string;
  stepAudioReferenceVoiceId: string;
  stepAudioPromptText: string;
  stepAudioUseCustomReference: boolean;
  stepAudioReferenceAudioPath: string;
  stepAudioReferenceText: string;
  stepAudioReferenceLabel: string;
  magpieVoice: string;
  magpieVoiceConfigs: NarrationMagpieVoiceConfig[];
  supertonicSpeed: number;
  supertonicNumSteps: number;
  supertonicNumThreads: number;
  supertonicVoiceConfigs: NarrationSupertonicVoiceConfig[];
}

export interface NarrationProfilePayload extends NarrationSettingsState {}

export interface NarrationGeminiVoice {
  name: string;
  gender: 'Male' | 'Female';
}

export interface NarrationGeminiModel {
  apiModel: string;
  label: string;
}

export interface NarrationGeminiInstructionLanguage {
  languageCode: string;
  languageName: string;
}

export interface NarrationEdgeVoiceLanguage {
  languageCode: string;
  languageName: string;
}

export interface NarrationEdgeVoiceOption {
  shortName: string;
  gender: string;
  friendlyName: string;
  locale: string;
}

export interface NarrationTtsProviderOption {
  method: NarrationTtsMethod;
  label: string;
}

export interface NarrationKokoroVoice {
  id: string;
  label: string;
  languageCode: string;
}

export interface NarrationMagpieVoice {
  id: string;
  label: string;
}

export interface NarrationSupertonicLanguage {
  languageCode: string;
  languageName: string;
}

export interface NarrationSupertonicVoice {
  id: string;
  label: string;
}

interface NarrationTtsMetadata {
  providers?: NarrationTtsProviderOption[];
  geminiVoices: NarrationGeminiVoice[];
  geminiModels: NarrationGeminiModel[];
  geminiInstructionLanguages: NarrationGeminiInstructionLanguage[];
  geminiSpeedOptions: string[];
  googleSpeedOptions: string[];
  kokoroVoices?: NarrationKokoroVoice[];
  kokoroVoiceLanguages?: NarrationGeminiInstructionLanguage[];
  magpieVoices?: NarrationMagpieVoice[];
  magpieVoiceLanguages?: NarrationGeminiInstructionLanguage[];
  supertonicLanguages?: NarrationSupertonicLanguage[];
  supertonicVoices?: NarrationSupertonicVoice[];
  stepAudioVoices?: NarrationStepAudioVoice[];
  stepAudioReferenceVoices?: NarrationStepAudioVoice[];
  edgeVoiceState?: 'idle' | 'loading' | 'loaded' | 'error';
  edgeVoiceError?: string | null;
  edgeVoiceLanguages?: NarrationEdgeVoiceLanguage[];
  edgeVoicesByLanguage?: Record<string, NarrationEdgeVoiceOption[]>;
  defaults: NarrationSettingsState;
}

const STORAGE_KEY = 'screen-record-narration-tts-v3';

const FALLBACK_DEFAULTS: NarrationSettingsState = {
  method: 'GeminiLive',
  geminiModel: '',
  geminiVoice: '',
  geminiSpeed: '',
  geminiInstruction: '',
  geminiLanguageConditions: [],
  googleSpeed: '',
  edgeVoice: '',
  edgePitch: 0,
  edgeRate: 0,
  edgeVoiceConfigs: [],
  kokoroVoice: 'af_heart',
  kokoroSpeed: 1,
  kokoroNumThreads: 2,
  kokoroVoiceConfigs: [],
  stepAudioVoice: 'default_en',
  stepAudioReferenceVoiceId: '',
  stepAudioPromptText: '',
  stepAudioUseCustomReference: false,
  stepAudioReferenceAudioPath: '',
  stepAudioReferenceText: '',
  stepAudioReferenceLabel: '',
  magpieVoice: '',
  magpieVoiceConfigs: [],
  supertonicSpeed: 1,
  supertonicNumSteps: 5,
  supertonicNumThreads: 2,
  supertonicVoiceConfigs: [],
};

function readStoredOverrides(): Partial<NarrationSettingsState> | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return null;
    return JSON.parse(raw) as Partial<NarrationSettingsState>;
  } catch {
    return null;
  }
}

function mergeWithDefaults(
  defaults: NarrationSettingsState,
  overrides: Partial<NarrationSettingsState> | null,
): NarrationSettingsState {
  const normalizedDefaults = {
    ...FALLBACK_DEFAULTS,
    ...defaults,
    geminiLanguageConditions: Array.isArray(defaults.geminiLanguageConditions)
      ? defaults.geminiLanguageConditions
      : FALLBACK_DEFAULTS.geminiLanguageConditions,
    edgeVoiceConfigs: Array.isArray(defaults.edgeVoiceConfigs)
      ? defaults.edgeVoiceConfigs
      : FALLBACK_DEFAULTS.edgeVoiceConfigs,
    kokoroVoiceConfigs: Array.isArray(defaults.kokoroVoiceConfigs)
      ? defaults.kokoroVoiceConfigs
      : FALLBACK_DEFAULTS.kokoroVoiceConfigs,
    magpieVoiceConfigs: Array.isArray(defaults.magpieVoiceConfigs)
      ? defaults.magpieVoiceConfigs
      : FALLBACK_DEFAULTS.magpieVoiceConfigs,
    supertonicVoiceConfigs: Array.isArray(defaults.supertonicVoiceConfigs)
      ? defaults.supertonicVoiceConfigs
      : FALLBACK_DEFAULTS.supertonicVoiceConfigs,
  };
  if (!overrides) return normalizedDefaults;
  return {
    ...normalizedDefaults,
    ...overrides,
    geminiLanguageConditions: Array.isArray(overrides.geminiLanguageConditions)
      ? overrides.geminiLanguageConditions
      : normalizedDefaults.geminiLanguageConditions,
    edgeVoiceConfigs: Array.isArray(overrides.edgeVoiceConfigs) && overrides.edgeVoiceConfigs.length
      ? overrides.edgeVoiceConfigs
      : normalizedDefaults.edgeVoiceConfigs,
    kokoroVoiceConfigs: Array.isArray(overrides.kokoroVoiceConfigs) && overrides.kokoroVoiceConfigs.length
      ? overrides.kokoroVoiceConfigs
      : normalizedDefaults.kokoroVoiceConfigs,
    magpieVoiceConfigs: Array.isArray(overrides.magpieVoiceConfigs) && overrides.magpieVoiceConfigs.length
      ? overrides.magpieVoiceConfigs
      : normalizedDefaults.magpieVoiceConfigs,
    supertonicVoiceConfigs: Array.isArray(overrides.supertonicVoiceConfigs) && overrides.supertonicVoiceConfigs.length
      ? overrides.supertonicVoiceConfigs
      : normalizedDefaults.supertonicVoiceConfigs,
  };
}

export function useNarrationSettings() {
  const overridesRef = useRef<Partial<NarrationSettingsState> | null>(readStoredOverrides());
  const [defaults, setDefaults] = useState<NarrationSettingsState>(FALLBACK_DEFAULTS);
  const [metadata, setMetadata] = useState<NarrationTtsMetadata | null>(null);
  const [settings, setSettings] = useState<NarrationSettingsState>(() =>
    mergeWithDefaults(FALLBACK_DEFAULTS, overridesRef.current),
  );

  useEffect(() => {
    let cancelled = false;
    let refreshTimer: number | null = null;
    const loadMetadata = () => invoke<NarrationTtsMetadata>('get_narration_tts_metadata', {})
      .then((meta) => {
        if (cancelled) return;
        setMetadata(meta);
        setDefaults(meta.defaults);
        setSettings(mergeWithDefaults(meta.defaults, overridesRef.current));
        if (meta.edgeVoiceState === 'loading') {
          refreshTimer = window.setTimeout(() => {
            refreshTimer = null;
            if (!cancelled) void loadMetadata();
          }, 1200);
        }
        return meta;
      })
      .catch((error) => {
        if (cancelled) return;
        // Leave fallback in place; the UI just won't list real voice options.
        console.warn('[Narration] Failed to load TTS metadata:', error);
      });
    void loadMetadata();
    return () => {
      cancelled = true;
      if (refreshTimer !== null) {
        window.clearTimeout(refreshTimer);
      }
    };
  }, []);

  useEffect(() => {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
    } catch {
      // ignore persistence failures
    }
  }, [settings]);

  const update = useCallback(<K extends keyof NarrationSettingsState>(
    key: K,
    value: NarrationSettingsState[K],
  ) => {
    setSettings((prev) => ({ ...prev, [key]: value }));
  }, []);

  const replace = useCallback((next: NarrationSettingsState) => {
    setSettings(next);
  }, []);

  const resetToDefaults = useCallback(() => {
    overridesRef.current = null;
    setSettings(defaults);
  }, [defaults]);

  const profile = useMemo<NarrationProfilePayload>(() => settings, [settings]);

  return {
    settings,
    update,
    replace,
    resetToDefaults,
    profile,
    metadata,
  };
}
