import { useEffect, useSyncExternalStore } from "react";
import type { TtsPlaygroundState } from "./types";

// Module-level mutable store. The Rust side sets state via window setters
// (sync_to_webview) and useStateStore subscribes via useSyncExternalStore.

const FALLBACK: TtsPlaygroundState = {
  theme: "dark",
  uiLanguage: "en",
  mode: "TtsClone",
  method: "GeminiLive",
  draftText: "",
  gemini: {
    model: "",
    voice: "",
    speed: "Normal",
    instruction: "",
    conditions: [],
  },
  edge: { pitch: 0, rate: 0, voices: [] },
  google: { speed: "Normal" },
  stepAudio: { reference: "" },
  magpie: { speed: 1, threads: 4, voices: [] },
  kokoro: { speed: 1, threads: 4, voices: [] },
  supertonic: { speed: 1, threads: 4, steps: 24, voices: [] },
  vieneu: { reference: "" },
  audioEdit: {
    sourcePath: "",
    sourceText: "",
    editType: "emotion",
    editInfo: "",
    targetText: "",
  },
  s2sTargetLanguage: "vi",
  player: {
    isGenerating: false,
    isExporting: false,
    isMicRecording: false,
    isPlaying: false,
    paused: false,
    positionSec: 0,
    status: "",
    recent: [],
  },
  catalogs: {
    geminiModels: [],
    geminiVoices: [],
    geminiInstructionLanguages: [],
    edgeVoicesByLanguage: {},
    edgeAvailableLanguages: [],
    magpieVoicesByLanguage: {},
    magpieAvailableLanguages: [],
    kokoroVoicesByLanguage: {},
    kokoroAvailableLanguages: [],
    supertonicVoicesByLanguage: {},
    supertonicAvailableLanguages: [],
    s2sLanguages: [],
    audioEditTasks: [],
    audioEditSubtasksByTask: {},
    paralinguisticTags: [],
    stepAudioReferences: [],
    vieneuReferences: [],
  },
  strings: {
    title: "TTS Playground",
    modeTtsClone: "TTS / Clone",
    modeAudioEdit: "Audio Edit",
    modeReferenceLibrary: "Reference Library",
    modeS2S: "S2S",
    methodLabel: "Method",
    methodGemini: "Gemini Live",
    methodEdge: "Edge TTS",
    methodGoogle: "Google Translate",
    methodStepAudio: "Step Audio EditX",
    methodMagpie: "NVIDIA Magpie-Multilingual 357M",
    methodKokoro: "Kokoro 82M v1.0",
    methodSupertonic: "Supertonic 3",
    methodVieneu: "VieNeu-TTS v2",
    textLabel: "Text",
    textHint: "Type or paste text to synthesize…",
    charCountTemplate: "{n} chars",
    generate: "Generate",
    clear: "Clear",
    cancel: "Cancel",
    generating: "Generating…",
    exporting: "Exporting MP3…",
    noAudio: "No audio yet — generate one above.",
    play: "Play",
    pause: "Pause",
    resume: "Resume",
    stop: "Stop",
    replay: "Replay",
    downloadWav: "WAV",
    downloadMp3: "MP3",
    recent: "Recent clips",
    voicePerLanguage: "Voice per language",
    addLanguage: "Add language",
    reset: "Reset",
    speedLabel: "Speed",
    speedSlow: "Slow",
    speedNormal: "Normal",
    speedFast: "Fast",
    pitchLabel: "Pitch",
    rateLabel: "Rate",
    threadsLabel: "Threads",
    qualityStepsLabel: "Quality steps",
    pickSource: "Pick source audio",
    useCurrent: "Use current clip",
    recordMic: "Record mic",
    stopMic: "Stop recording",
    noSource: "No source audio",
    sourceTranscript: "Source transcript",
    task: "Task",
    subtask: "Subtask",
    inlineSoundTag: "Inline sound tag",
    insertTag: "Insert tag",
    targetText: "Target text",
    referenceVoice: "Reference voice",
    referenceLibraryDesc: "Shared by Step Audio TTS, global TTS config, and narration.",
    referenceAdd: "+ Add reference",
    referenceLabel: "Label",
    referencePickAudio: "Pick audio",
    referenceAutoRecognize: "Auto recognize",
    referenceUsePlayground: "Use in playground",
    referenceUseGlobal: "Use globally",
    referenceNoAudio: "No reference audio selected",
    referenceExactTranscript: "Exact reference transcript",
    geminiModelLabel: "Model",
    instructionsLabel: "Instructions",
    instructionsHint: "Optional style instruction for Gemini Live",
    preview: "Preview",
    delete: "Delete",
    stepAudioDesc:
      "Supports Mandarin, English, Sichuanese, Cantonese, Japanese, and Korean.",
    vieneuDesc: "Vietnamese & English, local sidecar.",
    s2sTarget: "Target",
    referenceEmpty: "No reference voices saved yet.",
  },
};

// Defense-in-depth: the Rust payload is authoritative, but if it ever omits
// or mis-shapes a key (during dev iteration, a stale schema, or a future
// refactor), React rendering must not crash. Every push is shallow-merged
// onto the fallback so collections + nested settings keep their shape.
function normalize(input: Partial<TtsPlaygroundState> | TtsPlaygroundState): TtsPlaygroundState {
  const src = (input ?? {}) as Partial<TtsPlaygroundState>;
  const merged: TtsPlaygroundState = {
    ...FALLBACK,
    ...src,
    gemini: { ...FALLBACK.gemini, ...(src.gemini ?? {}) },
    edge: { ...FALLBACK.edge, ...(src.edge ?? {}) },
    google: { ...FALLBACK.google, ...(src.google ?? {}) },
    stepAudio: { ...FALLBACK.stepAudio, ...(src.stepAudio ?? {}) },
    magpie: { ...FALLBACK.magpie, ...(src.magpie ?? {}) },
    kokoro: { ...FALLBACK.kokoro, ...(src.kokoro ?? {}) },
    supertonic: { ...FALLBACK.supertonic, ...(src.supertonic ?? {}) },
    vieneu: { ...FALLBACK.vieneu, ...(src.vieneu ?? {}) },
    audioEdit: { ...FALLBACK.audioEdit, ...(src.audioEdit ?? {}) },
    player: { ...FALLBACK.player, ...(src.player ?? {}) },
    catalogs: { ...FALLBACK.catalogs, ...(src.catalogs ?? {}) },
    strings: { ...FALLBACK.strings, ...(src.strings ?? {}) },
  };
  return merged;
}

let current: TtsPlaygroundState = normalize(
  (typeof window !== "undefined" && window.__TTS_INITIAL_STATE__) || FALLBACK,
);
const listeners = new Set<() => void>();

function setState(next: TtsPlaygroundState) {
  current = normalize(next);
  listeners.forEach((l) => l());
}

function patchState(patch: Partial<TtsPlaygroundState>) {
  current = normalize({ ...current, ...patch });
  listeners.forEach((l) => l());
}

if (typeof window !== "undefined") {
  window.__TTS_SET_STATE__ = setState;
  window.__TTS_PATCH_STATE__ = patchState;
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

function getSnapshot(): TtsPlaygroundState {
  return current;
}

export function useTtsState(): TtsPlaygroundState {
  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
}

export function useThemeAttr() {
  const theme = useTtsState().theme;
  useEffect(() => {
    document.documentElement.dataset.theme = theme;
  }, [theme]);
}
