import en, { type Translations } from './en';
import vi from './vi';
import ko from './ko';

const locales: Record<string, Translations> = { en, vi, ko };

export function getTranslations(lang: string): Translations {
  return locales[lang] || locales.en;
}

export type { Translations };
