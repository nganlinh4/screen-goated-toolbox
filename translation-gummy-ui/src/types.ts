export type TranslationGummyProfile = {
  language: string;
  accent: string;
  tone: string;
};

export type HotkeyItem = {
  code: number;
  name: string;
  modifiers: number;
};

export type TranslationGummyState = {
  darkMode: boolean;
  statusLabel: string;
  connectionState: string;
  isRunning: boolean;
  dirty: boolean;
  canApply: boolean;
  canToggle: boolean;
  audioLevel: number;
  draft: {
    first: TranslationGummyProfile;
    second: TranslationGummyProfile;
  };
  hotkeys: HotkeyItem[];
  guideSeen: boolean;
  ttsModel: string;
  ttsVoice: string;
  hotkeyError?: string | null;
  lastError?: string | null;
  transcripts: Array<{
    id: number;
    role: "input" | "output" | "separator";
    text: string;
    isFinal: boolean;
    lang: string;
  }>;
  strings: {
    title: string;
    firstProfile: string;
    secondProfile: string;
    languageLabel: string;
    accentLabel: string;
    toneLabel: string;
    hotkeyLabel: string;
    setHotkey: string;
    clearHotkey: string;
    apply: string;
    start: string;
    stop: string;
    transcriptTitle: string;
    inputChip: string;
    outputChip: string;
    noTranscript: string;
    guide: string;
    guideOk: string;
    chatHistory: string;
    currentModel: string;
    currentVoice: string;
  };
};

declare global {
  interface Window {
    __TG_INITIAL_STATE__?: TranslationGummyState;
    __TG_SET_STATE?: (payload: TranslationGummyState) => void;
    invoke?: (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;
  }
}
