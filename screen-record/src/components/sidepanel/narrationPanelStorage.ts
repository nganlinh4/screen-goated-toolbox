import {
  DEFAULT_NARRATION_GROUP_TEXT_BUDGET,
  MAX_NARRATION_GROUP_TEXT_BUDGET,
  MIN_NARRATION_GROUP_TEXT_BUDGET,
} from '@/hooks/useSubtitleNarration';

const READ_UNSPLIT_SUBTITLES_KEY = 'screen-record-narration-read-unsplit-subtitles-v1';
const NARRATION_GROUP_TEXT_BUDGET_KEY = 'screen-record-narration-group-text-budget-v2';

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
