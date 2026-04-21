export interface SubtitleLanguageOption {
  value: string;
  label: string;
  keywords?: string[];
}

export const SUBTITLE_LANGUAGE_OPTIONS: SubtitleLanguageOption[] = [
  { value: 'auto', label: 'Auto', keywords: ['automatic', 'detect'] },
  { value: 'afr', label: 'Afrikaans' },
  { value: 'sq', label: 'Albanian', keywords: ['sqi'] },
  { value: 'ar', label: 'Arabic', keywords: ['ara'] },
  { value: 'az', label: 'Azerbaijani', keywords: ['aze'] },
  { value: 'be', label: 'Belarusian', keywords: ['bel'] },
  { value: 'bn', label: 'Bengali', keywords: ['ben'] },
  { value: 'bg', label: 'Bulgarian', keywords: ['bul'] },
  { value: 'my', label: 'Burmese', keywords: ['mya', 'myanmar'] },
  { value: 'ca', label: 'Catalan', keywords: ['cat'] },
  { value: 'zh', label: 'Chinese', keywords: ['zho'] },
  { value: 'cmn', label: 'Mandarin Chinese', keywords: ['mandarin'] },
  { value: 'hr', label: 'Croatian', keywords: ['hrv'] },
  { value: 'cs', label: 'Czech', keywords: ['ces'] },
  { value: 'da', label: 'Danish', keywords: ['dan'] },
  { value: 'nl', label: 'Dutch', keywords: ['nld'] },
  { value: 'en', label: 'English', keywords: ['eng'] },
  { value: 'eo', label: 'Esperanto', keywords: ['epo'] },
  { value: 'et', label: 'Estonian', keywords: ['est'] },
  { value: 'eu', label: 'Basque', keywords: ['eus'] },
  { value: 'fa', label: 'Persian', keywords: ['pes', 'farsi'] },
  { value: 'fi', label: 'Finnish', keywords: ['fin'] },
  { value: 'fr', label: 'French', keywords: ['fra', 'fre'] },
  { value: 'ka', label: 'Georgian', keywords: ['kat'] },
  { value: 'de', label: 'German', keywords: ['deu', 'ger'] },
  { value: 'el', label: 'Greek', keywords: ['ell'] },
  { value: 'gu', label: 'Gujarati', keywords: ['guj'] },
  { value: 'he', label: 'Hebrew', keywords: ['heb'] },
  { value: 'hi', label: 'Hindi', keywords: ['hin'] },
  { value: 'hu', label: 'Hungarian', keywords: ['hun'] },
  { value: 'id', label: 'Indonesian', keywords: ['ind'] },
  { value: 'it', label: 'Italian', keywords: ['ita'] },
  { value: 'ja', label: 'Japanese', keywords: ['jpn'] },
  { value: 'kn', label: 'Kannada', keywords: ['kan'] },
  { value: 'ko', label: 'Korean', keywords: ['kor'] },
  { value: 'la', label: 'Latin', keywords: ['lat'] },
  { value: 'lv', label: 'Latvian', keywords: ['lav'] },
  { value: 'lt', label: 'Lithuanian', keywords: ['lit'] },
  { value: 'ml', label: 'Malayalam', keywords: ['mal'] },
  { value: 'mr', label: 'Marathi', keywords: ['mar'] },
  { value: 'mk', label: 'Macedonian', keywords: ['mkd'] },
  { value: 'ne', label: 'Nepali', keywords: ['nep'] },
  { value: 'nb', label: 'Norwegian Bokmål', keywords: ['nob', 'bokmal', 'bokmål'] },
  { value: 'nn', label: 'Norwegian Nynorsk', keywords: ['nno'] },
  { value: 'or', label: 'Oriya', keywords: ['odia'] },
  { value: 'pa', label: 'Punjabi', keywords: ['pan'] },
  { value: 'pl', label: 'Polish', keywords: ['pol'] },
  { value: 'pt', label: 'Portuguese', keywords: ['por'] },
  { value: 'ro', label: 'Romanian', keywords: ['ron'] },
  { value: 'ru', label: 'Russian', keywords: ['rus'] },
  { value: 'sr', label: 'Serbian', keywords: ['srp'] },
  { value: 'si', label: 'Sinhala', keywords: ['sin'] },
  { value: 'sk', label: 'Slovak', keywords: ['slk'] },
  { value: 'sl', label: 'Slovenian', keywords: ['slv'] },
  { value: 'so', label: 'Somali', keywords: ['som'] },
  { value: 'es', label: 'Spanish', keywords: ['spa'] },
  { value: 'sv', label: 'Swedish', keywords: ['swe'] },
  { value: 'ta', label: 'Tamil', keywords: ['tam'] },
  { value: 'tl', label: 'Tagalog', keywords: ['tgl'] },
  { value: 'te', label: 'Telugu', keywords: ['tel'] },
  { value: 'th', label: 'Thai', keywords: ['tha'] },
  { value: 'tr', label: 'Turkish', keywords: ['tur'] },
  { value: 'uk', label: 'Ukrainian', keywords: ['ukr'] },
  { value: 'ur', label: 'Urdu', keywords: ['urd'] },
  { value: 'uz', label: 'Uzbek', keywords: ['uzb'] },
  { value: 'vi', label: 'Vietnamese', keywords: ['vie'] },
  { value: 'yi', label: 'Yiddish', keywords: ['yid'] },
];

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
