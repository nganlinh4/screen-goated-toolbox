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

const WHISPER_LANGUAGE_ENTRIES: Array<[value: string, label: string, keywords?: string[]]> = [
  ['en', 'English'],
  ['zh', 'Chinese'],
  ['de', 'German'],
  ['es', 'Spanish'],
  ['ru', 'Russian'],
  ['ko', 'Korean'],
  ['fr', 'French'],
  ['ja', 'Japanese'],
  ['pt', 'Portuguese'],
  ['tr', 'Turkish'],
  ['pl', 'Polish'],
  ['ca', 'Catalan'],
  ['nl', 'Dutch'],
  ['ar', 'Arabic'],
  ['sv', 'Swedish'],
  ['it', 'Italian'],
  ['id', 'Indonesian'],
  ['hi', 'Hindi'],
  ['fi', 'Finnish'],
  ['vi', 'Vietnamese'],
  ['he', 'Hebrew'],
  ['uk', 'Ukrainian'],
  ['el', 'Greek'],
  ['ms', 'Malay'],
  ['cs', 'Czech'],
  ['ro', 'Romanian'],
  ['da', 'Danish'],
  ['hu', 'Hungarian'],
  ['ta', 'Tamil'],
  ['no', 'Norwegian'],
  ['th', 'Thai'],
  ['ur', 'Urdu'],
  ['hr', 'Croatian'],
  ['bg', 'Bulgarian'],
  ['lt', 'Lithuanian'],
  ['la', 'Latin'],
  ['mi', 'Maori'],
  ['ml', 'Malayalam'],
  ['cy', 'Welsh'],
  ['sk', 'Slovak'],
  ['te', 'Telugu'],
  ['fa', 'Persian', ['farsi']],
  ['lv', 'Latvian'],
  ['bn', 'Bengali'],
  ['sr', 'Serbian'],
  ['az', 'Azerbaijani'],
  ['sl', 'Slovenian'],
  ['kn', 'Kannada'],
  ['et', 'Estonian'],
  ['mk', 'Macedonian'],
  ['br', 'Breton'],
  ['eu', 'Basque'],
  ['is', 'Icelandic'],
  ['hy', 'Armenian'],
  ['ne', 'Nepali'],
  ['mn', 'Mongolian'],
  ['bs', 'Bosnian'],
  ['kk', 'Kazakh'],
  ['sq', 'Albanian'],
  ['sw', 'Swahili'],
  ['gl', 'Galician'],
  ['mr', 'Marathi'],
  ['pa', 'Punjabi'],
  ['si', 'Sinhala'],
  ['km', 'Khmer'],
  ['sn', 'Shona'],
  ['yo', 'Yoruba'],
  ['so', 'Somali'],
  ['af', 'Afrikaans'],
  ['oc', 'Occitan'],
  ['ka', 'Georgian'],
  ['be', 'Belarusian'],
  ['tg', 'Tajik'],
  ['sd', 'Sindhi'],
  ['gu', 'Gujarati'],
  ['am', 'Amharic'],
  ['yi', 'Yiddish'],
  ['lo', 'Lao'],
  ['uz', 'Uzbek'],
  ['fo', 'Faroese'],
  ['ht', 'Haitian Creole'],
  ['ps', 'Pashto'],
  ['tk', 'Turkmen'],
  ['nn', 'Nynorsk', ['norwegian nynorsk']],
  ['mt', 'Maltese'],
  ['sa', 'Sanskrit'],
  ['lb', 'Luxembourgish'],
  ['my', 'Myanmar', ['burmese']],
  ['bo', 'Tibetan'],
  ['tl', 'Tagalog', ['filipino']],
  ['mg', 'Malagasy'],
  ['as', 'Assamese'],
  ['tt', 'Tatar'],
  ['haw', 'Hawaiian'],
  ['ln', 'Lingala'],
  ['ha', 'Hausa'],
  ['ba', 'Bashkir'],
  ['jw', 'Javanese'],
  ['su', 'Sundanese'],
  ['yue', 'Cantonese', ['yue']],
];

const QWEN_LANGUAGE_ENTRIES: Array<[value: string, label: string, keywords?: string[]]> = [
  ['ar', 'Arabic'],
  ['cs', 'Czech'],
  ['da', 'Danish'],
  ['de', 'German'],
  ['el', 'Greek'],
  ['en', 'English'],
  ['es', 'Spanish'],
  ['fa', 'Persian', ['farsi']],
  ['fi', 'Finnish'],
  ['fil', 'Filipino', ['tagalog']],
  ['fr', 'French'],
  ['hi', 'Hindi'],
  ['hu', 'Hungarian'],
  ['id', 'Indonesian'],
  ['it', 'Italian'],
  ['ja', 'Japanese'],
  ['ko', 'Korean'],
  ['mk', 'Macedonian'],
  ['ms', 'Malay'],
  ['nl', 'Dutch'],
  ['pl', 'Polish'],
  ['pt', 'Portuguese'],
  ['ro', 'Romanian'],
  ['ru', 'Russian'],
  ['sv', 'Swedish'],
  ['th', 'Thai'],
  ['tr', 'Turkish'],
  ['vi', 'Vietnamese'],
  ['yue', 'Cantonese (Yue)', ['cantonese']],
  ['zh', 'Chinese'],
  ['qwen-dialect-anhui', 'Chinese Dialect: Anhui'],
  ['qwen-dialect-dongbei', 'Chinese Dialect: Dongbei'],
  ['qwen-dialect-fujian', 'Chinese Dialect: Fujian'],
  ['qwen-dialect-gansu', 'Chinese Dialect: Gansu'],
  ['qwen-dialect-guizhou', 'Chinese Dialect: Guizhou'],
  ['qwen-dialect-hebei', 'Chinese Dialect: Hebei'],
  ['qwen-dialect-henan', 'Chinese Dialect: Henan'],
  ['qwen-dialect-hubei', 'Chinese Dialect: Hubei'],
  ['qwen-dialect-hunan', 'Chinese Dialect: Hunan'],
  ['qwen-dialect-jiangxi', 'Chinese Dialect: Jiangxi'],
  ['qwen-dialect-ningxia', 'Chinese Dialect: Ningxia'],
  ['qwen-dialect-shandong', 'Chinese Dialect: Shandong'],
  ['qwen-dialect-shaanxi', 'Chinese Dialect: Shaanxi'],
  ['qwen-dialect-shanxi', 'Chinese Dialect: Shanxi'],
  ['qwen-dialect-sichuan', 'Chinese Dialect: Sichuan'],
  ['qwen-dialect-tianjin', 'Chinese Dialect: Tianjin'],
  ['qwen-dialect-yunnan', 'Chinese Dialect: Yunnan'],
  ['qwen-dialect-zhejiang', 'Chinese Dialect: Zhejiang'],
  ['qwen-dialect-cantonese-hk', 'Chinese Dialect: Cantonese (Hong Kong)', ['hong kong cantonese']],
  ['qwen-dialect-cantonese-gd', 'Chinese Dialect: Cantonese (Guangdong)', ['guangdong cantonese']],
  ['qwen-dialect-wu', 'Chinese Dialect: Wu language', ['shanghainese']],
  ['qwen-dialect-minnan', 'Chinese Dialect: Minnan language', ['hokkien']],
];

function buildOptions(entries: Array<[value: string, label: string, keywords?: string[]]>) {
  return [
    AUTO_LANGUAGE_OPTION,
    ...entries.map(([value, label, keywords]) => ({ value, label, keywords })),
  ];
}

export const SUBTITLE_LANGUAGE_OPTIONS_GROQ: SubtitleLanguageOption[] =
  buildOptions(WHISPER_LANGUAGE_ENTRIES);

export const SUBTITLE_LANGUAGE_OPTIONS_QWEN: SubtitleLanguageOption[] =
  buildOptions(QWEN_LANGUAGE_ENTRIES);

export const SUBTITLE_LANGUAGE_OPTIONS: SubtitleLanguageOption[] = SUBTITLE_LANGUAGE_OPTIONS_GROQ;

export function getSubtitleLanguageOptionsForMethod(method: string): SubtitleLanguageOption[] {
  if (method === 'qwen-local-0-6b' || method === 'qwen-local-1-7b') {
    return SUBTITLE_LANGUAGE_OPTIONS_QWEN;
  }
  return SUBTITLE_LANGUAGE_OPTIONS_GROQ;
}

const SUBTITLE_LANGUAGE_OPTIONS_BY_VALUE = new Map(
  [...SUBTITLE_LANGUAGE_OPTIONS_GROQ, ...SUBTITLE_LANGUAGE_OPTIONS_QWEN]
    .map((option) => [option.value, option]),
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
