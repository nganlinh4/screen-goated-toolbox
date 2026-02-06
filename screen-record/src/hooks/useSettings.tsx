import { createContext, useContext, useState, useEffect } from 'react';
import { getTranslations, type Translations } from '@/i18n';

interface SettingsState {
  theme: 'dark' | 'light';
  lang: string;
  t: Translations;
}

// Read initial values set synchronously by Rust init script
const initialTheme: 'dark' | 'light' =
  (window as any).__SR_INITIAL_THEME__ === 'light' ? 'light' : 'dark';
const initialLang: string =
  (window as any).__SR_INITIAL_LANG__ || 'en';

const defaultState: SettingsState = {
  theme: initialTheme,
  lang: initialLang,
  t: getTranslations(initialLang),
};

export const SettingsContext = createContext<SettingsState>(defaultState);

export function useSettings() {
  return useContext(SettingsContext);
}

export function useSettingsProvider(): SettingsState {
  const [theme, setTheme] = useState<'dark' | 'light'>(initialTheme);
  const [lang, setLang] = useState(initialLang);
  const [t, setT] = useState<Translations>(getTranslations(initialLang));

  useEffect(() => {
    if (theme === 'dark') {
      document.documentElement.classList.add('dark');
    } else {
      document.documentElement.classList.remove('dark');
    }
  }, [theme]);

  useEffect(() => {
    const handler = (e: MessageEvent) => {
      if (e.data?.type === 'sr-set-settings') {
        if (e.data.theme) setTheme(e.data.theme);
        if (e.data.lang) {
          setLang(e.data.lang);
          setT(getTranslations(e.data.lang));
        }
      }
    };
    window.addEventListener('message', handler);
    return () => window.removeEventListener('message', handler);
  }, []);

  return { theme, lang, t };
}
