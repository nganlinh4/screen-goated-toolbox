import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@/lib/ipc';
import { createPersistedSetting } from '@/lib/persistedState';

const NARRATION_TTS_METHODS = ['GeminiLive', 'GoogleTranslate', 'EdgeTTS'] as const;

export type NarrationTtsMethod = (typeof NARRATION_TTS_METHODS)[number];

function isNarrationTtsMethod(value: unknown): value is NarrationTtsMethod {
  return NARRATION_TTS_METHODS.some((method) => method === value);
}

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

export interface NarrationSettingsState {
  method: NarrationTtsMethod;
  geminiModel: string;
  geminiVoice: string;
  geminiSpeed: string;
  geminiInstruction: string;
  geminiLanguageConditions: NarrationLanguageCondition[];
  geminiParallelRequests: number;
  geminiS2sParallelRequests: number;
  googleSpeed: string;
  edgeVoice: string;
  edgePitch: number;
  edgeRate: number;
  edgeVoiceConfigs: NarrationEdgeVoiceConfig[];
}

export type NarrationProfilePayload = NarrationSettingsState;

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

interface NarrationTtsMetadata {
  providers?: NarrationTtsProviderOption[];
  geminiVoices: NarrationGeminiVoice[];
  geminiModels: NarrationGeminiModel[];
  geminiInstructionLanguages: NarrationGeminiInstructionLanguage[];
  geminiSpeedOptions: string[];
  googleSpeedOptions: string[];
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
  geminiParallelRequests: 2,
  geminiS2sParallelRequests: 3,
  googleSpeed: '',
  edgeVoice: '',
  edgePitch: 0,
  edgeRate: 0,
  edgeVoiceConfigs: [],
};

const narrationSettingsStore = createPersistedSetting<Partial<NarrationSettingsState> | null>(
  STORAGE_KEY,
  {
    parse: (raw) => (raw ? (JSON.parse(raw) as Partial<NarrationSettingsState>) : null),
    serialize: (value) => (value === null ? null : JSON.stringify(value)),
    fallback: null,
  },
);

function readStoredOverrides(): Partial<NarrationSettingsState> | null {
  return narrationSettingsStore.getInitial();
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
  };
  if (!overrides) return normalizedDefaults;
  return {
    ...normalizedDefaults,
    ...overrides,
    // A provider saved by an earlier version may no longer exist.
    method: isNarrationTtsMethod(overrides.method) ? overrides.method : normalizedDefaults.method,
    geminiLanguageConditions: Array.isArray(overrides.geminiLanguageConditions)
      ? overrides.geminiLanguageConditions
      : normalizedDefaults.geminiLanguageConditions,
    edgeVoiceConfigs: Array.isArray(overrides.edgeVoiceConfigs) && overrides.edgeVoiceConfigs.length
      ? overrides.edgeVoiceConfigs
      : normalizedDefaults.edgeVoiceConfigs,
  };
}

export function useNarrationSettings() {
  // The user's choices over the host defaults. Every edit lands here too, so a
  // metadata (re)load merges fresh defaults under them instead of reverting them.
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
    narrationSettingsStore.persist(settings);
  }, [settings]);

  const update = useCallback(<K extends keyof NarrationSettingsState>(
    key: K,
    value: NarrationSettingsState[K],
  ) => {
    overridesRef.current = { ...overridesRef.current, [key]: value };
    setSettings((prev) => ({ ...prev, [key]: value }));
  }, []);

  const replace = useCallback((next: NarrationSettingsState) => {
    overridesRef.current = next;
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
