import { describe, it, expect, beforeEach, vi } from "vitest";
import { render, screen, cleanup, fireEvent, act } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { App } from "../src/App";
import type { TtsPlaygroundState } from "../src/types";

function baseState(): TtsPlaygroundState {
  return {
    theme: "dark",
    uiLanguage: "en",
    mode: "TtsClone",
    method: "GeminiLive",
    draftText: "Hello!",
    gemini: {
      model: "gemini-3.1-flash-live-preview",
      voice: "Aoede",
      speed: "Normal",
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
    s2sTargetLanguage: "Vietnamese",
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
      geminiModels: [{ value: "gemini-3.1-flash-live-preview", label: "Live 3.1" }],
      geminiVoices: [{ value: "Aoede", label: "Aoede", gender: "female" }],
      geminiInstructionLanguages: [],
      edgeVoicesByLanguage: {},
      edgeAvailableLanguages: [],
      magpieVoicesByLanguage: {},
      magpieAvailableLanguages: [],
      kokoroVoicesByLanguage: {},
      kokoroAvailableLanguages: [],
      supertonicVoicesByLanguage: {},
      supertonicAvailableLanguages: [],
      s2sLanguages: [{ value: "Vietnamese", label: "Vietnamese" }],
      audioEditTasks: [{ value: "emotion", label: "emotion" }],
      audioEditSubtasksByTask: { emotion: [{ value: "happy", label: "happy" }] },
      paralinguisticTags: ["(laughter)"],
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
      methodMagpie: "Magpie",
      methodKokoro: "Kokoro",
      methodSupertonic: "Supertonic 3",
      methodVieneu: "VieNeu",
      textLabel: "Text",
      textHint: "Hint",
      charCountTemplate: "{n} chars",
      generate: "Generate",
      clear: "Clear",
      cancel: "Cancel",
      generating: "Generating…",
      exporting: "Exporting",
      noAudio: "No audio",
      play: "Play",
      pause: "Pause",
      resume: "Resume",
      stop: "Stop",
      replay: "Replay",
      downloadWav: "WAV",
      downloadMp3: "MP3",
      recent: "Recent",
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
      qualityStepsLabel: "Steps",
      pickSource: "Pick source",
      useCurrent: "Use current",
      recordMic: "Record",
      stopMic: "Stop mic",
      noSource: "No source",
      sourceTranscript: "Source",
      task: "Task",
      subtask: "Subtask",
      inlineSoundTag: "Inline tag",
      insertTag: "Insert",
      targetText: "Target",
      referenceVoice: "Reference",
      geminiModelLabel: "Model",
      instructionsLabel: "Instructions",
      instructionsHint: "Style hint",
      preview: "Preview",
      delete: "Delete",
      stepAudioDesc: "Desc",
      vieneuDesc: "Desc",
      s2sTarget: "Target",
      referenceEmpty: "Empty",
    },
  };
}

function pushState(patch: Partial<TtsPlaygroundState>) {
  const next = { ...baseState(), ...patch };
  act(() => {
    window.__TTS_SET_STATE__?.(next);
  });
}

describe("Provider switching + state-driven rendering", () => {
  beforeEach(() => {
    cleanup();
    // Reset to fallback for the next render
    act(() => {
      window.__TTS_SET_STATE__?.(baseState());
    });
  });

  it("renders the GeminiLive panel by default", () => {
    render(<App />);
    expect(screen.getByText("Model")).toBeInTheDocument();
    expect(screen.getByText("Reference")).toBeInTheDocument();
  });

  it("renders the EdgeTTS panel when method switches", () => {
    render(<App />);
    pushState({ method: "EdgeTTS" });
    expect(screen.getByText("Pitch")).toBeInTheDocument();
    expect(screen.getByText("Rate")).toBeInTheDocument();
  });

  it("renders the GoogleTranslate panel with Speed control only", () => {
    render(<App />);
    pushState({ method: "GoogleTranslate" });
    expect(screen.getAllByText("Google Translate").length).toBeGreaterThan(0);
    expect(screen.getByText("Speed")).toBeInTheDocument();
  });

  it("renders the AudioEdit panel when mode switches", () => {
    render(<App />);
    pushState({ mode: "AudioEdit" });
    expect(screen.getByText("Pick source")).toBeInTheDocument();
    expect(screen.getByText("Task")).toBeInTheDocument();
  });

  it("renders the player + exports when a current clip exists", () => {
    render(<App />);
    pushState({
      player: {
        isGenerating: false,
        isExporting: false,
        isMicRecording: false,
        isPlaying: false,
        paused: false,
        positionSec: 0,
        status: "Ready",
        recent: [],
        current: {
          id: "clip-1",
          text: "Hello!",
          voiceLabel: "Aoede",
          createdLabel: "12:00:00",
          durationSec: 1.5,
          sampleRate: 24000,
        },
      },
    });
    expect(screen.getByText("WAV")).toBeInTheDocument();
    expect(screen.getByText("MP3")).toBeInTheDocument();
    expect(screen.getByText("Aoede")).toBeInTheDocument();
  });

  it("swaps Generate button for Generating + Cancel link when busy", () => {
    render(<App />);
    pushState({
      player: {
        isGenerating: true,
        isExporting: false,
        isMicRecording: false,
        isPlaying: false,
        paused: false,
        positionSec: 0,
        status: "Generating…",
        recent: [],
      },
    });
    expect(screen.getByText("Generating…")).toBeInTheDocument();
    expect(screen.getByText("Cancel")).toBeInTheDocument();
  });

  it("logs a warning when invoke is unavailable (non-WRY env)", () => {
    delete (window as any).invoke;
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    render(<App />);
    fireEvent.click(screen.getByText("Generate"));
    expect(warn).toHaveBeenCalledWith(
      expect.stringContaining("invoke called outside WRY"),
      expect.anything(),
    );
    warn.mockRestore();
  });
});
