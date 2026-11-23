// Thin typed wrappers around the WRY IPC bridge. Falls back to a no-op when
// running outside the WebView (tests, vite dev preview) so the React tree
// renders without crashing.

import type { TtsPlaygroundState } from "./types";
import { editLocally } from "./state";

function patchKey(scope: string, patch: object): string {
  return `${scope}:${Object.keys(patch).sort().join(",")}`;
}

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
  // The host never echoes set_draft_text, but any other event (a generation
  // finishing, playback ending) pushes its whole state, which may predate the
  // latest keystroke; keep that keystroke over such pushes until it is handled.
  setDraftText: (text: string) =>
    editLocally(
      "draftText",
      (state) => ({ ...state, draftText: text }),
      () => invoke("set_draft_text", { text }),
    ),
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
  setRecentLimit: (limit: number) => invoke("set_recent_limit", { limit }),
  clearRecent: () => invoke("clear_recent"),
  previewVoice: (speaker: string) => invoke("preview_voice", { speaker }),
  resetProvider: (provider: string) => invoke("reset_provider", { provider }),

  // Provider settings — generic patches keyed by provider.
  patchGemini: (patch: Partial<TtsPlaygroundState["gemini"]>) =>
    editLocally(
      patchKey("gemini", patch),
      (state) => ({ ...state, gemini: { ...state.gemini, ...patch } }),
      () => invoke("patch_gemini", { patch }),
    ),
  patchEdge: (patch: Partial<TtsPlaygroundState["edge"]>) =>
    invoke("patch_edge", { patch }),
  patchGoogle: (patch: Partial<TtsPlaygroundState["google"]>) =>
    invoke("patch_google", { patch }),

  setS2sTargetLanguage: (language: string) =>
    invoke("set_s2s_target_language", { language }),

  pickSourceAudio: () => invoke<string | null>("pick_source_audio"),
  startMicRecording: () => invoke("start_mic_recording"),
  stopMicRecording: () => invoke("stop_mic_recording"),
  closeWindow: () => invoke("close_window"),
  minimizeWindow: () => invoke("minimize_window"),
  startDrag: () => invoke("start_drag"),
};
