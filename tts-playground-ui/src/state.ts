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
  sourceAudio: { sourcePath: "" },
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
  recentLimit: 10,
  catalogs: {
    geminiModels: [],
    geminiVoices: [],
    geminiInstructionLanguages: [],
    edgeVoicesByLanguage: {},
    edgeAvailableLanguages: [],
    s2sLanguages: [],
  },
  strings: {
    title: "TTS Playground",
    modeTtsClone: "TTS / Clone",
    modeS2S: "S2S",
    methodLabel: "Method",
    methodGemini: "Gemini Live",
    methodEdge: "Edge TTS",
    methodGoogle: "Google Translate",
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
    recentLimit: "Max items",
    clearRecent: "Clear all",
    deleteRecent: "Delete",
    voicePerLanguage: "Voice per language",
    addLanguage: "Add language",
    reset: "Reset",
    speedLabel: "Speed",
    speedSlow: "Slow",
    speedNormal: "Normal",
    speedFast: "Fast",
    pitchLabel: "Pitch",
    rateLabel: "Rate",
    pickSource: "Pick source audio",
    recordMic: "Record mic",
    stopMic: "Stop recording",
    noSource: "No source audio",
    referenceVoice: "Reference voice",
    geminiModelLabel: "Model",
    instructionsLabel: "Instructions",
    instructionsHint: "Optional style instruction for Gemini Live",
    preview: "Preview",
    delete: "Delete",
    s2sTarget: "Target",
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
    sourceAudio: { ...FALLBACK.sourceAudio, ...(src.sourceAudio ?? {}) },
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

type LocalEdit = { seq: number; apply: (state: TtsPlaygroundState) => TtsPlaygroundState };

// Typed edits the host has not acknowledged yet. The host replies to each
// command and then echoes its whole state, so echoes of earlier keystrokes
// arrive while newer ones are still in flight; replaying these edits over
// every echo keeps controlled inputs from snapping back and dropping input.
const localEdits = new Map<string, LocalEdit>();
let localEditSeq = 0;

function withLocalEdits(state: TtsPlaygroundState): TtsPlaygroundState {
  let next = state;
  localEdits.forEach((edit) => {
    next = normalize(edit.apply(next));
  });
  return next;
}

function setState(next: TtsPlaygroundState) {
  current = withLocalEdits(normalize(next));
  listeners.forEach((l) => l());
}

/**
 * Shows a field edit immediately and keeps it over host echoes until the host
 * has replied to the latest edit of that field.
 */
export function editLocally<T>(
  key: string,
  apply: (state: TtsPlaygroundState) => TtsPlaygroundState,
  send: () => Promise<T>,
): Promise<T> {
  const seq = ++localEditSeq;
  localEdits.set(key, { seq, apply });
  current = normalize(apply(current));
  listeners.forEach((l) => l());
  const settle = () => {
    if (localEdits.get(key)?.seq === seq) localEdits.delete(key);
  };
  return send().then(
    (value) => {
      settle();
      return value;
    },
    (error: unknown) => {
      settle();
      throw error;
    },
  );
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
