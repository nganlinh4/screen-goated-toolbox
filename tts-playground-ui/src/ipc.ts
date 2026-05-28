// Thin typed wrappers around the WRY IPC bridge. Falls back to a no-op when
// running outside the WebView (tests, vite dev preview) so the React tree
// renders without crashing.

import type { TtsPlaygroundState } from "./types";

export async function invoke<T = unknown>(
  cmd: string,
  args: Record<string, unknown> = {},
): Promise<T> {
  if (!window.invoke) {
    console.warn(`[tts-playground] invoke called outside WRY: ${cmd}`, args);
    return undefined as T;
  }
  return window.invoke<T>(cmd, args);
}

export const ttsApi = {
  setMode: (mode: TtsPlaygroundState["mode"]) =>
    invoke("set_mode", { mode }),
  setMethod: (method: TtsPlaygroundState["method"]) =>
    invoke("set_method", { method }),
  setDraftText: (text: string) => {
    window.__TTS_PATCH_STATE__?.({ draftText: text });
    return invoke("set_draft_text", { text });
  },
  generate: () => invoke("generate"),
  cancelGeneration: () => invoke("cancel_generation"),
  clear: () => invoke("clear"),
  play: () => invoke("play"),
  pause: () => invoke("pause"),
  stop: () => invoke("stop"),
  replay: () => invoke("replay"),
  seek: (sec: number) => invoke("seek", { sec }),
  downloadWav: () => invoke<string | null>("download_wav"),
  downloadMp3: () => invoke<string | null>("download_mp3"),
  playRecent: (id: string) => invoke("play_recent", { id }),
  deleteRecent: (id: string) => invoke("delete_recent", { id }),
  previewVoice: (speaker: string) => invoke("preview_voice", { speaker }),
  resetProvider: (provider: string) => invoke("reset_provider", { provider }),

  // Provider settings — generic patches keyed by provider.
  patchGemini: (patch: Partial<TtsPlaygroundState["gemini"]>) =>
    invoke("patch_gemini", { patch }),
  patchEdge: (patch: Partial<TtsPlaygroundState["edge"]>) =>
    invoke("patch_edge", { patch }),
  patchGoogle: (patch: Partial<TtsPlaygroundState["google"]>) =>
    invoke("patch_google", { patch }),
  patchStepAudio: (patch: Partial<TtsPlaygroundState["stepAudio"]>) =>
    invoke("patch_step_audio", { patch }),
  patchMagpie: (patch: Partial<TtsPlaygroundState["magpie"]>) =>
    invoke("patch_magpie", { patch }),
  patchKokoro: (patch: Partial<TtsPlaygroundState["kokoro"]>) =>
    invoke("patch_kokoro", { patch }),
  patchSupertonic: (patch: Partial<TtsPlaygroundState["supertonic"]>) =>
    invoke("patch_supertonic", { patch }),
  patchVieneu: (patch: Partial<TtsPlaygroundState["vieneu"]>) =>
    invoke("patch_vieneu", { patch }),
  patchAudioEdit: (patch: Partial<TtsPlaygroundState["audioEdit"]>) =>
    invoke("patch_audio_edit", { patch }),

  setS2sTargetLanguage: (language: string) =>
    invoke("set_s2s_target_language", { language }),

  pickSourceAudio: () => invoke<string | null>("pick_source_audio"),
  startMicRecording: () => invoke("start_mic_recording"),
  stopMicRecording: () => invoke("stop_mic_recording"),
  startReferenceMic: (id: string) => invoke("start_reference_mic", { id }),
  stopReferenceMic: () => invoke("stop_reference_mic"),
  useCurrentAsSource: () => invoke("use_current_as_source"),
  addReference: () => invoke("add_reference"),
  updateReference: (
    id: string,
    patch: { label?: string; transcript?: string },
  ) => invoke("update_reference", { id, ...patch }),
  deleteReference: (id: string) => invoke("delete_reference", { id }),
  pickReferenceAudio: (id: string) =>
    invoke<string | null>("pick_reference_audio", { id }),
  recognizeReference: (id: string) => invoke("recognize_reference", { id }),
  playReference: (id: string) => invoke("play_reference", { id }),
  useReference: (id: string, target: "playground" | "global") =>
    invoke("use_reference", { id, target }),

  closeWindow: () => invoke("close_window"),
  minimizeWindow: () => invoke("minimize_window"),
  startDrag: () => invoke("start_drag"),
};
