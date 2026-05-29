/**
 * E2E user-journey test.
 *
 * Drives the React app exactly the way the WRY shell does in production:
 * a real `window.invoke` bridge that captures every IPC call, and the
 * `__TTS_SET_STATE__` global wired by Rust to push canonical state into
 * the UI. Every action a user can perform from the mini-app must round
 * trip through this bridge — if a button silently no-ops, this suite
 * catches it.
 */
import { describe, it, expect, beforeEach, vi } from "vitest";
import {
  render,
  screen,
  cleanup,
  fireEvent,
  act,
  within,
} from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { App } from "../src/App";
import type { TtsPlaygroundState } from "../src/types";

type InvokeCall = { cmd: string; args: unknown };

function baseState(): TtsPlaygroundState {
  return {
    theme: "dark",
    uiLanguage: "en",
    mode: "TtsClone",
    method: "GeminiLive",
    draftText: "",
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
      stepAudioReferences: [
        { id: "voice-a", name: "Reference A" },
        { id: "voice-b", name: "Reference B" },
      ],
      vieneuReferences: [{ id: "vn-1", name: "VN Default" }],
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

describe("WRY round-trip user journey", () => {
  let calls: InvokeCall[] = [];

  beforeEach(() => {
    cleanup();
    calls = [];
    // Install a deterministic invoke that records every command. This is
    // exactly the contract the Rust side fulfills.
    (window as any).invoke = vi.fn(async (cmd: string, args: unknown) => {
      calls.push({ cmd, args });
      return null;
    });
    act(() => {
      window.__TTS_SET_STATE__?.(baseState());
    });
  });

  it("drives the full TTS → playback → export journey", () => {
    render(<App />);

    // 1. User types into the draft text box → set_draft_text fires.
    fireEvent.change(screen.getByPlaceholderText("Hint"), {
      target: { value: "Hello world" },
    });
    expect(calls.some((c) => c.cmd === "set_draft_text")).toBe(true);

    // 2. Clicks Generate → generate fires.
    fireEvent.click(screen.getByText("Generate"));
    expect(calls.some((c) => c.cmd === "generate")).toBe(true);

    // 3. Backend pushes a generating state; UI swaps to Cancel link.
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
    fireEvent.click(screen.getByText("Cancel"));
    expect(calls.some((c) => c.cmd === "cancel_generation")).toBe(true);

    // 4. Backend pushes a ready clip; player controls appear.
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
          text: "Hello world",
          voiceLabel: "Aoede",
          createdLabel: "12:00:00",
          durationSec: 2.0,
          sampleRate: 24000,
        },
      },
    });
    fireEvent.click(screen.getByText("Play"));
    expect(calls.some((c) => c.cmd === "play")).toBe(true);

    // 5. Backend transitions to playing → user pauses, then replays.
    pushState({
      player: {
        isGenerating: false,
        isExporting: false,
        isMicRecording: false,
        isPlaying: true,
        paused: false,
        positionSec: 0.5,
        status: "Playing",
        recent: [],
        current: {
          id: "clip-1",
          text: "Hello world",
          voiceLabel: "Aoede",
          createdLabel: "12:00:00",
          durationSec: 2.0,
          sampleRate: 24000,
        },
      },
    });
    fireEvent.click(screen.getByText("Pause"));
    expect(calls.some((c) => c.cmd === "pause")).toBe(true);
    fireEvent.click(screen.getByText("Replay"));
    expect(calls.some((c) => c.cmd === "replay")).toBe(true);

    // 6. Exports.
    fireEvent.click(screen.getByText("WAV"));
    expect(calls.some((c) => c.cmd === "download_wav")).toBe(true);
    fireEvent.click(screen.getByText("MP3"));
    expect(calls.some((c) => c.cmd === "download_mp3")).toBe(true);
  });

  it("drives the Reference Library picker", () => {
    render(<App />);
    pushState({ mode: "ReferenceLibrary" });

    // Two Step Audio references are listed; click the second one.
    fireEvent.click(screen.getByText("Reference B"));
    const patch = calls.find((c) => c.cmd === "patch_step_audio");
    expect(patch).toBeDefined();
    expect((patch?.args as any).patch).toMatchObject({ reference: "voice-b" });

    // The VieNeu list is rendered too.
    fireEvent.click(screen.getByText("VN Default"));
    const vieneu = calls.find((c) => c.cmd === "patch_vieneu");
    expect(vieneu).toBeDefined();
    expect((vieneu?.args as any).patch).toMatchObject({ reference: "vn-1" });
  });

  it("drives the mic record / stop toggle in AudioEdit", () => {
    render(<App />);
    pushState({ mode: "AudioEdit" });

    fireEvent.click(screen.getByText("Record"));
    expect(calls.some((c) => c.cmd === "start_mic_recording")).toBe(true);

    pushState({
      mode: "AudioEdit",
      player: {
        isGenerating: false,
        isExporting: false,
        isMicRecording: true,
        isPlaying: false,
        paused: false,
        positionSec: 0,
        status: "Recording…",
        recent: [],
      },
    });
    fireEvent.click(screen.getByText("Stop mic"));
    expect(calls.some((c) => c.cmd === "stop_mic_recording")).toBe(true);
  });

  it("hides the no-op speed/threads sliders on Magpie", () => {
    render(<App />);
    pushState({ method: "MagpieMultilingual" });

    // Locate the Magpie card by its heading. The method picker trigger also
    // shows "Magpie", so we target the card title (an <h3> heading role) to
    // disambiguate.
    const card = screen
      .getByRole("heading", { name: "Magpie" })
      .closest("div");
    expect(card).toBeTruthy();
    // Magpie no longer advertises Speed / Threads (the backend struct
    // doesn't carry them) — Kokoro and Supertonic still do.
    expect(within(card as HTMLElement).queryByText("Speed")).toBeNull();
    expect(within(card as HTMLElement).queryByText("Threads")).toBeNull();
  });
});
