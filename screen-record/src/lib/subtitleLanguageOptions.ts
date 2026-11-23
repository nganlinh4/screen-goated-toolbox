import whisperLanguages from '../../../catalog/whisper-languages.json';

export interface SubtitleLanguageOption {
  value: string;
  label: string;
  keywords?: string[];
}

const AUTO_LANGUAGE_OPTION: SubtitleLanguageOption = {
  value: 'auto',
  label: 'Auto',
  keywords: ['automatic', 'detect'],
};

export const SUBTITLE_LANGUAGE_OPTIONS: SubtitleLanguageOption[] =
  [AUTO_LANGUAGE_OPTION, ...whisperLanguages];

const SUBTITLE_LANGUAGE_OPTIONS_BY_VALUE = new Map(
  SUBTITLE_LANGUAGE_OPTIONS.map((option) => [option.value, option]),
);

export function getSubtitleLanguageOption(
  value: string | null | undefined,
): SubtitleLanguageOption | null {
  if (!value) return null;
  return SUBTITLE_LANGUAGE_OPTIONS_BY_VALUE.get(value) ?? null;
}

export function getSubtitleLanguageLabel(value: string | null | undefined): string {
  if (!value) return 'Translation';
  return getSubtitleLanguageOption(value)?.label ?? value;
}
