const LANGUAGE_6393_TO_PRIMARY: Record<string, string> = {
  arb: 'ar',
  cmn: 'zh',
  deu: 'de',
  eng: 'en',
  fra: 'fr',
  hin: 'hi',
  ita: 'it',
  jpn: 'ja',
  kor: 'ko',
  por: 'pt',
  spa: 'es',
  vie: 'vi',
  yue: 'zh',
  zho: 'zh',
};

const LANGUAGE_PRIMARY_TO_6393: Record<string, string> = {
  ar: 'arb',
  de: 'deu',
  en: 'eng',
  es: 'spa',
  fr: 'fra',
  hi: 'hin',
  it: 'ita',
  ja: 'jpn',
  ko: 'kor',
  pt: 'por',
  vi: 'vie',
  zh: 'cmn',
};

export interface NarrationLanguageDetectionResponse {
  languageCode?: string | null;
  sample?: string;
}

export function normalizeLanguagePrimary(code: string | null | undefined) {
  const normalized = code?.trim().toLowerCase();
  if (!normalized || normalized === 'auto') return null;
  const base = normalized.split(/[-_]/)[0];
  return LANGUAGE_6393_TO_PRIMARY[base] ?? base;
}

export function normalizeLanguage6393(code: string | null | undefined) {
  const primary = normalizeLanguagePrimary(code);
  if (!primary) return null;
  if (primary.length === 3) return primary;
  return LANGUAGE_PRIMARY_TO_6393[primary] ?? primary;
}

export function languageMatches(
  candidate: string | null | undefined,
  detectedCode: string,
) {
  const candidatePrimary = normalizeLanguagePrimary(candidate);
  const candidate6393 = normalizeLanguage6393(candidate);
  const detectedPrimary = normalizeLanguagePrimary(detectedCode);
  const detected6393 = normalizeLanguage6393(detectedCode);
  return !!candidatePrimary && (
    candidatePrimary === detectedPrimary
    || candidate6393 === detected6393
  );
}

export function kokoroVoiceLanguageForCondition(languageCode: string) {
  switch (languageCode.toLowerCase()) {
    case 'eng':
      return 'en-us';
    case 'cmn':
    case 'zho':
      return 'zh';
    case 'jpn':
      return 'ja';
    case 'spa':
      return 'es';
    case 'fra':
      return 'fr';
    case 'hin':
      return 'hi';
    case 'ita':
      return 'it';
    case 'por':
      return 'pt-br';
    default:
      return '';
  }
}
