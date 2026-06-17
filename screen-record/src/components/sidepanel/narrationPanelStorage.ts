import {
  DEFAULT_NARRATION_GROUP_TEXT_BUDGET,
  MAX_NARRATION_GROUP_TEXT_BUDGET,
  MIN_NARRATION_GROUP_TEXT_BUDGET,
} from '@/hooks/useSubtitleNarration';

const READ_UNSPLIT_SUBTITLES_KEY = 'screen-record-narration-read-unsplit-subtitles-v1';
const NARRATION_GROUP_TEXT_BUDGET_KEY = 'screen-record-narration-group-text-budget-v2';
const NARRATION_MODE_KEY = 'screen-record-narration-mode-v1';
const DIRECT_VOICE_METHOD_KEY = 'screen-record-direct-voice-method-v1';

export type StoredNarrationMode = 'subtitles' | 's2s';
export type StoredDirectVoiceMethod = 's2s' | 'gemini-translate';

export function getInitialNarrationGroupTextBudget() {
  try {
    const raw = Number(localStorage.getItem(NARRATION_GROUP_TEXT_BUDGET_KEY));
    if (
      Number.isFinite(raw) &&
      raw >= MIN_NARRATION_GROUP_TEXT_BUDGET &&
      raw <= MAX_NARRATION_GROUP_TEXT_BUDGET
    ) {
      return Math.round(raw);
    }
  } catch {
    // ignore persistence failures
  }
  return DEFAULT_NARRATION_GROUP_TEXT_BUDGET;
}

export function getInitialReadUnsplitSubtitles() {
  try {
    const raw = localStorage.getItem(READ_UNSPLIT_SUBTITLES_KEY);
    return raw === null ? true : raw === 'true';
  } catch {
    return true;
  }
}

export function getInitialNarrationMode(hasSubtitles: boolean): StoredNarrationMode {
  try {
    const raw = localStorage.getItem(NARRATION_MODE_KEY);
    if (raw === 'subtitles' && hasSubtitles) return 'subtitles';
    if (raw === 's2s') return 's2s';
  } catch {
    // ignore persistence failures
  }
  return hasSubtitles ? 'subtitles' : 's2s';
}

export function getInitialDirectVoiceMethod(): StoredDirectVoiceMethod {
  try {
    const raw = localStorage.getItem(DIRECT_VOICE_METHOD_KEY);
    if (raw === 's2s' || raw === 'gemini-translate') return raw;
  } catch {
    // ignore persistence failures
  }
  return 's2s';
}

export function persistReadUnsplitSubtitles(value: boolean) {
  try {
    localStorage.setItem(READ_UNSPLIT_SUBTITLES_KEY, String(value));
  } catch {
    // ignore persistence failures
  }
}

export function persistNarrationGroupTextBudget(value: number) {
  try {
    localStorage.setItem(NARRATION_GROUP_TEXT_BUDGET_KEY, String(value));
  } catch {
    // ignore persistence failures
  }
}

export function persistNarrationMode(value: StoredNarrationMode) {
  try {
    localStorage.setItem(NARRATION_MODE_KEY, value);
  } catch {
    // ignore persistence failures
  }
}

export function persistDirectVoiceMethod(value: StoredDirectVoiceMethod) {
  try {
    localStorage.setItem(DIRECT_VOICE_METHOD_KEY, value);
  } catch {
    // ignore persistence failures
  }
}
