import type { SubtitleSource } from '@/lib/subtitleGenerationPlan';
import { DEFAULT_GEMINI_SUBTITLE_PROMPT } from '@/lib/geminiSubtitlePrompt';
import { createPersistedSetting } from '@/lib/persistedState';
import type { SubtitleMethod, SubtitleMethodCapability } from './subtitleGenerationTypes';

const SUBTITLE_SOURCE_KEY = 'screen-record-subtitle-source-v1';
const SUBTITLE_METHOD_KEY = 'screen-record-subtitle-method-v1';
const SUBTITLE_LANGUAGE_HINT_KEY = 'screen-record-subtitle-language-hint-v1';
const SUBTITLE_GEMINI_PROMPT_KEY = 'screen-record-subtitle-gemini-prompt-v1';
const SUBTITLE_GROQ_VOCABULARY_KEY = 'screen-record-subtitle-groq-vocabulary-v1';
const SUBTITLE_AUTO_SPLIT_KEY = 'screen-record-subtitle-auto-split-v1';
const SUBTITLE_AUTO_SPLIT_MAX_UNITS_KEY = 'screen-record-subtitle-auto-split-max-units-v1';

export const DEFAULT_SUBTITLE_METHOD_CAPABILITIES: SubtitleMethodCapability[] = [
  { method: 'groq-whisper-accurate', available: true, reason: null },
  { method: 'groq-whisper-large-v3-turbo', available: true, reason: null },
  { method: 'gemini-3-1-flash-lite', available: true, reason: null },
  { method: 'gemini-3-flash-preview', available: true, reason: null },
  { method: 'qwen-local-0-6b', available: true, reason: null },
  { method: 'qwen-local-1-7b', available: true, reason: null },
  { method: 'parakeet-tdt-0-6b-v3', available: true, reason: null },
];

export const DEFAULT_SUBTITLE_AUTO_SPLIT_MAX_UNITS = 8;

function isSubtitleSource(value: string | null): value is SubtitleSource {
  return value === 'video'
    || value === 'mic'
    || value === 'audio'
    || value?.startsWith('audio:') === true;
}

function normalizeLegacySubtitleSource(value: string | null): string | null {
  if (value === 'music') return 'audio';
  if (value?.startsWith('music:')) return `audio:${value.slice('music:'.length)}`;
  return value;
}

function isSubtitleMethod(value: string): value is SubtitleMethod {
  return (
    value === 'groq-whisper-accurate' ||
    value === 'groq-whisper-large-v3-turbo' ||
    value === 'gemini-3-1-flash-lite' ||
    value === 'gemini-3-flash-preview' ||
    value === 'qwen-local-0-6b' ||
    value === 'qwen-local-1-7b' ||
    value === 'parakeet-tdt-0-6b-v3'
  );
}

function normalizeStoredSubtitleMethod(value: string | null): SubtitleMethod | null {
  return value && isSubtitleMethod(value) ? value : null;
}

export function isQwenLocalSubtitleMethod(method: SubtitleMethod) {
  return method === 'qwen-local-0-6b' || method === 'qwen-local-1-7b';
}

export function isParakeetTdtSubtitleMethod(method: SubtitleMethod) {
  return method === 'parakeet-tdt-0-6b-v3';
}

const subtitleSourceSetting = createPersistedSetting<SubtitleSource>(SUBTITLE_SOURCE_KEY, {
  parse: (raw) => {
    const normalized = normalizeLegacySubtitleSource(raw);
    return isSubtitleSource(normalized) ? normalized : 'video';
  },
  serialize: (value) => value,
  fallback: 'video',
});

const subtitleMethodSetting = createPersistedSetting<SubtitleMethod>(SUBTITLE_METHOD_KEY, {
  parse: (raw) => normalizeStoredSubtitleMethod(raw) ?? 'groq-whisper-accurate',
  serialize: (value) => value,
  fallback: 'groq-whisper-accurate',
});

const subtitleLanguageHintSetting = createPersistedSetting<string>(SUBTITLE_LANGUAGE_HINT_KEY, {
  parse: (raw) => (raw && raw.trim() ? raw : 'auto'),
  serialize: (value) => value.trim() || 'auto',
  fallback: 'auto',
});

const geminiPromptSetting = createPersistedSetting<string>(SUBTITLE_GEMINI_PROMPT_KEY, {
  parse: (raw) => (raw?.trim() ? raw : DEFAULT_GEMINI_SUBTITLE_PROMPT),
  serialize: (value) => value,
  fallback: DEFAULT_GEMINI_SUBTITLE_PROMPT,
});

const groqVocabularySetting = createPersistedSetting<string[]>(SUBTITLE_GROQ_VOCABULARY_KEY, {
  parse: (raw) => {
    const parsed = JSON.parse(raw ?? '[]');
    if (Array.isArray(parsed)) {
      return parsed
        .filter((entry): entry is string => typeof entry === 'string')
        .map((entry) => entry.trim())
        .filter(Boolean);
    }
    return [];
  },
  serialize: (value) => JSON.stringify(value),
  fallback: [],
});

const autoSplitEnabledSetting = createPersistedSetting<boolean>(SUBTITLE_AUTO_SPLIT_KEY, {
  parse: (raw) => (raw === null ? true : raw === 'true'),
  serialize: (value) => String(value),
  fallback: true,
});

const autoSplitMaxUnitsSetting = createPersistedSetting<number>(SUBTITLE_AUTO_SPLIT_MAX_UNITS_KEY, {
  parse: (raw) => {
    const value = Number(raw);
    if (Number.isFinite(value) && value >= 3 && value <= 24) {
      return Math.round(value);
    }
    return DEFAULT_SUBTITLE_AUTO_SPLIT_MAX_UNITS;
  },
  serialize: (value) => String(value),
  fallback: DEFAULT_SUBTITLE_AUTO_SPLIT_MAX_UNITS,
});

export function getInitialSubtitleSource(): SubtitleSource {
  return subtitleSourceSetting.getInitial();
}

export function getInitialSubtitleMethod(): SubtitleMethod {
  return subtitleMethodSetting.getInitial();
}

export function getInitialSubtitleLanguageHint(): string {
  return subtitleLanguageHintSetting.getInitial();
}

export function getInitialGeminiPrompt(): string {
  return geminiPromptSetting.getInitial();
}

export function getInitialGroqVocabulary(): string[] {
  return groqVocabularySetting.getInitial();
}

export function getInitialAutoSplitEnabled() {
  return autoSplitEnabledSetting.getInitial();
}

export function getInitialAutoSplitMaxUnits() {
  return autoSplitMaxUnitsSetting.getInitial();
}

export function persistSubtitleSource(value: SubtitleSource) {
  subtitleSourceSetting.persist(value);
}

export function persistSubtitleMethod(value: SubtitleMethod) {
  subtitleMethodSetting.persist(value);
}

export function persistSubtitleLanguageHint(value: string) {
  subtitleLanguageHintSetting.persist(value);
}

export function persistGeminiPrompt(value: string) {
  geminiPromptSetting.persist(value);
}

export function persistGroqVocabulary(value: string[]) {
  groqVocabularySetting.persist(value);
}

export function persistAutoSplitEnabled(value: boolean) {
  autoSplitEnabledSetting.persist(value);
}

export function persistAutoSplitMaxUnits(value: number) {
  autoSplitMaxUnitsSetting.persist(value);
}
