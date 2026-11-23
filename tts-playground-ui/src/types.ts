// Shared TS types that mirror the Rust-side IPC payloads. Keep keys in
// snake_case where the backend serializes that way; otherwise camelCase to
// match the JS norm.

export type TtsMethod =
  | "GeminiLive"
  | "EdgeTTS"
  | "GoogleTranslate";

export type TtsMode = "SpeechToSpeech" | "TtsClone";

export type SpeedKind = "Slow" | "Normal" | "Fast";

export type GeminiVoiceCondition = {
  language: string;
  name: string;
  instruction: string;
};

export type GeminiSettings = {
  model: string;
  voice: string;
  speed: SpeedKind;
  instruction: string;
  conditions: GeminiVoiceCondition[];
};

export type EdgeVoiceConfig = {
  language: string;
  voice: string;
};

export type EdgeSettings = {
  pitch: number;
  rate: number;
  voices: EdgeVoiceConfig[];
};

export type GoogleSettings = {
  speed: SpeedKind;
};

export type SourceAudioSettings = {
  sourcePath: string;
};

export type CurrentClip = {
  id: string;
  text: string;
  voiceLabel: string;
  createdLabel: string;
  durationSec: number;
  sampleRate: number;
};

export type RecentClip = {
  id: string;
  text: string;
  voiceLabel: string;
  createdLabel: string;
  durationSec: number;
};

export type PlayerState = {
  isGenerating: boolean;
  isExporting: boolean;
  isMicRecording: boolean;
  isPlaying: boolean;
  paused: boolean;
  positionSec: number;
  status: string;
  error?: string;
  current?: CurrentClip;
  recent: RecentClip[];
};

export type LocaleStrings = {
  title: string;
  modeTtsClone: string;
  modeS2S: string;
  methodLabel: string;
  methodGemini: string;
  methodEdge: string;
  methodGoogle: string;
  textLabel: string;
  textHint: string;
  charCountTemplate: string;
  generate: string;
  clear: string;
  cancel: string;
  generating: string;
  exporting: string;
  noAudio: string;
  play: string;
  pause: string;
  resume: string;
  stop: string;
  replay: string;
  downloadWav: string;
  downloadMp3: string;
  recent: string;
  recentLimit: string;
  clearRecent: string;
  deleteRecent: string;
  voicePerLanguage: string;
  addLanguage: string;
  reset: string;
  speedLabel: string;
  speedSlow: string;
  speedNormal: string;
  speedFast: string;
  pitchLabel: string;
  rateLabel: string;
  pickSource: string;
  recordMic: string;
  stopMic: string;
  noSource: string;
  referenceVoice: string;
  geminiModelLabel: string;
  instructionsLabel: string;
  instructionsHint: string;
  preview: string;
  delete: string;
  s2sTarget: string;
};

export type CatalogOption<V extends string = string> = {
  value: V;
  label: string;
};

export type ProviderCatalogs = {
  geminiModels: CatalogOption[];
  geminiVoices: { value: string; label: string; gender: "male" | "female" }[];
  geminiInstructionLanguages: CatalogOption[];
  edgeVoicesByLanguage: Record<string, CatalogOption[]>;
  edgeAvailableLanguages: CatalogOption[];
  s2sLanguages: CatalogOption[];
};

export type TtsPlaygroundState = {
  theme: "dark" | "light";
  uiLanguage: string;
  mode: TtsMode;
  method: TtsMethod;
  draftText: string;
  gemini: GeminiSettings;
  edge: EdgeSettings;
  google: GoogleSettings;
  sourceAudio: SourceAudioSettings;
  s2sTargetLanguage: string;
  player: PlayerState;
  recentLimit: number;
  catalogs: ProviderCatalogs;
  strings: LocaleStrings;
};

declare global {
  interface Window {
    __TTS_INITIAL_STATE__?: TtsPlaygroundState;
    __TTS_SET_STATE__?: (next: TtsPlaygroundState) => void;
    __TTS_PATCH_STATE__?: (patch: Partial<TtsPlaygroundState>) => void;
    invoke?: <T = unknown>(
      cmd: string,
      args?: Record<string, unknown>,
    ) => Promise<T>;
    isWry?: boolean;
  }
}
