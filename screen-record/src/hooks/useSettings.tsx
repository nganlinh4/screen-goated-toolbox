import { createContext, useContext, useState, useEffect, useLayoutEffect } from 'react';
import { getTranslations, type Translations } from '@/i18n';
import { projectManager } from '@/lib/projectManager';

interface SettingsState {
  theme: 'dark' | 'light';
  lang: string;
  t: Translations;
}

// Read initial values set synchronously by Rust init script
type RecorderSettingsWindow = Window & {
  __SR_INITIAL_THEME__?: string;
  __SR_INITIAL_LANG__?: string;
};

const settingsWindow = window as RecorderSettingsWindow;
const initialTheme: 'dark' | 'light' =
  settingsWindow.__SR_INITIAL_THEME__ === 'light' ? 'light' : 'dark';
const initialLang = settingsWindow.__SR_INITIAL_LANG__ || 'en';

if (typeof document !== 'undefined') {
  if (initialTheme === 'dark') {
    document.documentElement.classList.add('dark');
  } else {
    document.documentElement.classList.remove('dark');
  }
  document.documentElement.lang = initialLang;
}

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

  useLayoutEffect(() => {
    if (theme === 'dark') {
      document.documentElement.classList.add('dark');
    } else {
      document.documentElement.classList.remove('dark');
    }
  }, [theme]);

  useLayoutEffect(() => {
    document.documentElement.lang = lang;
  }, [lang]);

  useEffect(() => {
    const handler = (e: MessageEvent) => {
      if (e.data?.type === 'sr-set-settings') {
        if (e.data.theme) setTheme(e.data.theme);
        if (e.data.lang) {
          setLang(e.data.lang);
          setT(getTranslations(e.data.lang));
        }
        if (typeof e.data.projectLimit === 'number') {
          projectManager.applyHostLimit(e.data.projectLimit);
        }
        if (typeof e.data.uploadLimit === 'number') {
          window.dispatchEvent(new CustomEvent('sr-upload-limit', { detail: e.data.uploadLimit }));
        }
      }
    };
    window.addEventListener('message', handler);
    return () => window.removeEventListener('message', handler);
  }, []);

  return { theme, lang, t };
}
