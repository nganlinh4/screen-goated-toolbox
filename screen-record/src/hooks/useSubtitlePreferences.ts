import { useCallback, useEffect, useRef, useState } from 'react';
import { SUBTITLE_LANGUAGE_OPTIONS } from '@/lib/subtitleLanguageOptions';
import { getInitialAutoSplitEnabled, getInitialAutoSplitMaxUnits, getInitialGeminiPrompt, getInitialGroqVocabulary,
  getInitialSubtitleLanguageHint, getInitialSubtitleSource, persistAutoSplitEnabled, persistAutoSplitMaxUnits,
  persistGeminiPrompt, persistGroqVocabulary, persistSubtitleLanguageHint, persistSubtitleSource } from './subtitleGenerationStorage';

export function useSubtitlePreferences() {
  const [sourceType, setSourceType] = useState(getInitialSubtitleSource);
  const [languageHint, setLanguageHint] = useState(getInitialSubtitleLanguageHint);
  const [geminiPrompt, setGeminiPrompt] = useState(getInitialGeminiPrompt);
  const [groqVocabulary, setGroqVocabulary] = useState<string[]>(getInitialGroqVocabulary);
  const [autoSplitSubtitles, setAutoSplitSubtitles] = useState(getInitialAutoSplitEnabled);
  const [autoSplitMaxUnits, setAutoSplitMaxUnitsState] = useState(getInitialAutoSplitMaxUnits);
  const autoSplitSubtitlesRef = useRef(autoSplitSubtitles);
  const autoSplitMaxUnitsRef = useRef(autoSplitMaxUnits);
  useEffect(() => { persistSubtitleSource(sourceType); }, [sourceType]);
  useEffect(() => { persistSubtitleLanguageHint(languageHint); }, [languageHint]);
  useEffect(() => {
    if (!SUBTITLE_LANGUAGE_OPTIONS.some((option) => option.value === languageHint)) setLanguageHint('auto');
  }, [languageHint]);
  useEffect(() => { persistGeminiPrompt(geminiPrompt); }, [geminiPrompt]);
  useEffect(() => { persistGroqVocabulary(groqVocabulary); }, [groqVocabulary]);
  useEffect(() => { autoSplitSubtitlesRef.current = autoSplitSubtitles; persistAutoSplitEnabled(autoSplitSubtitles); }, [autoSplitSubtitles]);
  useEffect(() => { autoSplitMaxUnitsRef.current = autoSplitMaxUnits; persistAutoSplitMaxUnits(autoSplitMaxUnits); }, [autoSplitMaxUnits]);
  const setAutoSplitMaxUnits = useCallback((value: number) => setAutoSplitMaxUnitsState(Math.min(24, Math.max(3, Math.round(value)))), []);
  return { sourceType, setSourceType, languageHint, setLanguageHint, geminiPrompt, setGeminiPrompt, groqVocabulary,
    setGroqVocabulary, autoSplitSubtitles, setAutoSplitSubtitles, autoSplitMaxUnits, setAutoSplitMaxUnits,
    autoSplitSubtitlesRef, autoSplitMaxUnitsRef };
}
