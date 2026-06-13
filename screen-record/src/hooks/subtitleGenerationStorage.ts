import type { SubtitleSource } from '@/lib/subtitleGenerationPlan';
import { DEFAULT_GEMINI_SUBTITLE_PROMPT } from '@/lib/geminiSubtitlePrompt';
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

export function getInitialSubtitleSource(): SubtitleSource {
  try {
    const raw = normalizeLegacySubtitleSource(localStorage.getItem(SUBTITLE_SOURCE_KEY));
    if (isSubtitleSource(raw)) {
      return raw;
    }
  } catch {
    // ignore persistence failures
  }
  return 'video';
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

export function getInitialSubtitleMethod(): SubtitleMethod {
  try {
    const normalized = normalizeStoredSubtitleMethod(localStorage.getItem(SUBTITLE_METHOD_KEY));
    if (normalized) {
      return normalized;
    }
  } catch {
    // ignore persistence failures
  }
  return 'groq-whisper-accurate';
}

export function getInitialSubtitleLanguageHint(): string {
  try {
    const raw = localStorage.getItem(SUBTITLE_LANGUAGE_HINT_KEY);
    if (raw && raw.trim()) {
      return raw;
    }
  } catch {
    // ignore persistence failures
  }
  return 'auto';
}

export function getInitialGeminiPrompt(): string {
  try {
    const storedPrompt = localStorage.getItem(SUBTITLE_GEMINI_PROMPT_KEY);
    return storedPrompt?.trim() ? storedPrompt : DEFAULT_GEMINI_SUBTITLE_PROMPT;
  } catch {
    // ignore persistence failures
  }
  return DEFAULT_GEMINI_SUBTITLE_PROMPT;
}

export function getInitialGroqVocabulary(): string[] {
  try {
    const parsed = JSON.parse(localStorage.getItem(SUBTITLE_GROQ_VOCABULARY_KEY) ?? '[]');
    if (Array.isArray(parsed)) {
      return parsed
        .filter((entry): entry is string => typeof entry === 'string')
        .map((entry) => entry.trim())
        .filter(Boolean);
    }
  } catch {
    // ignore persistence failures
  }
  return [];
}

export function getInitialAutoSplitEnabled() {
  try {
    const raw = localStorage.getItem(SUBTITLE_AUTO_SPLIT_KEY);
    return raw === null ? true : raw === 'true';
  } catch {
    return true;
  }
}

export function getInitialAutoSplitMaxUnits() {
  try {
    const raw = Number(localStorage.getItem(SUBTITLE_AUTO_SPLIT_MAX_UNITS_KEY));
    if (Number.isFinite(raw) && raw >= 3 && raw <= 24) {
      return Math.round(raw);
    }
  } catch {
    // ignore persistence failures
  }
  return DEFAULT_SUBTITLE_AUTO_SPLIT_MAX_UNITS;
}

export function persistSubtitleSource(value: SubtitleSource) {
  localStorage.setItem(SUBTITLE_SOURCE_KEY, value);
}

export function persistSubtitleMethod(value: SubtitleMethod) {
  localStorage.setItem(SUBTITLE_METHOD_KEY, value);
}

export function persistSubtitleLanguageHint(value: string) {
  localStorage.setItem(SUBTITLE_LANGUAGE_HINT_KEY, value.trim() || 'auto');
}

export function persistGeminiPrompt(value: string) {
  localStorage.setItem(SUBTITLE_GEMINI_PROMPT_KEY, value);
}

export function persistGroqVocabulary(value: string[]) {
  localStorage.setItem(SUBTITLE_GROQ_VOCABULARY_KEY, JSON.stringify(value));
}

export function persistAutoSplitEnabled(value: boolean) {
  localStorage.setItem(SUBTITLE_AUTO_SPLIT_KEY, String(value));
}

export function persistAutoSplitMaxUnits(value: number) {
  localStorage.setItem(SUBTITLE_AUTO_SPLIT_MAX_UNITS_KEY, String(value));
}
