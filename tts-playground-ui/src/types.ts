// Shared TS types that mirror the Rust-side IPC payloads. Keep keys in
// snake_case where the backend serializes that way; otherwise camelCase to
// match the JS norm.

export type TtsMethod =
  | "GeminiLive"
  | "EdgeTTS"
  | "GoogleTranslate"
  | "StepAudioEditX"
  | "MagpieMultilingual"
  | "Kokoro"
  | "Supertonic"
  | "VieneuTts";

export type TtsMode =
  | "SpeechToSpeech"
  | "TtsClone"
  | "AudioEdit"
  | "ReferenceLibrary";

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

export type LocalVoiceConfig = {
  language: string;
  voice: string;
};

export type LocalProviderSettings = {
  speed: number;
  threads: number;
  steps?: number; // Supertonic only
  voices: LocalVoiceConfig[];
};

export type ReferenceVoice = {
  id: string;
  name: string;
  audioPath: string;
  transcript: string;
};

export type StepAudioSettings = {
  reference: string;
};

export type VieneuSettings = {
  reference: string;
};

export type AudioEditSettings = {
  sourcePath: string;
  sourceText: string;
  editType: string; // "emotion" | "style" | "speed" | "denoise" | "vad" | "paralinguistic"
  editInfo: string;
  targetText: string;
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
  modeAudioEdit: string;
  modeReferenceLibrary: string;
  modeS2S: string;
  methodLabel: string;
  methodGemini: string;
  methodEdge: string;
  methodGoogle: string;
  methodStepAudio: string;
  methodMagpie: string;
  methodKokoro: string;
  methodSupertonic: string;
  methodVieneu: string;
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
  voicePerLanguage: string;
  addLanguage: string;
  reset: string;
  speedLabel: string;
  speedSlow: string;
  speedNormal: string;
  speedFast: string;
  pitchLabel: string;
  rateLabel: string;
  threadsLabel: string;
  qualityStepsLabel: string;
  pickSource: string;
  useCurrent: string;
  recordMic: string;
  stopMic: string;
  noSource: string;
  sourceTranscript: string;
  task: string;
  subtask: string;
  inlineSoundTag: string;
  insertTag: string;
  targetText: string;
  referenceVoice: string;
  referenceLibraryDesc: string;
  referenceAdd: string;
  referenceLabel: string;
  referencePickAudio: string;
  referenceAutoRecognize: string;
  referenceUsePlayground: string;
  referenceUseGlobal: string;
  referenceNoAudio: string;
  referenceExactTranscript: string;
  geminiModelLabel: string;
  instructionsLabel: string;
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
  magpieVoicesByLanguage: Record<string, CatalogOption[]>;
  magpieAvailableLanguages: CatalogOption[];
  kokoroVoicesByLanguage: Record<string, CatalogOption[]>;
  kokoroAvailableLanguages: CatalogOption[];
  supertonicVoicesByLanguage: Record<string, CatalogOption[]>;
  supertonicAvailableLanguages: CatalogOption[];
  s2sLanguages: CatalogOption[];
  audioEditTasks: CatalogOption[];
  audioEditSubtasksByTask: Record<string, CatalogOption[]>;
  paralinguisticTags: string[];
  stepAudioReferences: ReferenceVoice[];
  vieneuReferences: ReferenceVoice[];
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
  stepAudio: StepAudioSettings;
  magpie: LocalProviderSettings;
  kokoro: LocalProviderSettings;
  supertonic: LocalProviderSettings;
  vieneu: VieneuSettings;
  audioEdit: AudioEditSettings;
  s2sTargetLanguage: string;
  player: PlayerState;
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
