type IpcCall = {
  cmd: string;
  args?: Record<string, unknown>;
  at: number;
};

type TestWindow = Window & {
  isWry?: boolean;
  invoke?: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
  ipc?: {
    postMessage: (message: string) => void;
  };
  __SGT_TEST_IPC_CALLS__?: IpcCall[];
};

export function isScreenRecordTestHarnessEnabled() {
  if (typeof window === "undefined") return false;
  const params = new URLSearchParams(window.location.search);
  return params.get("sgtTestHarness") === "1";
}

function defaultResponse(cmd: string, args?: Record<string, unknown>): unknown {
  switch (cmd) {
    case "get_hotkeys":
    case "get_monitors":
    case "get_windows":
    case "take_pending_audio_drop_actions":
    case "take_pending_subtitle_drop_actions":
    case "take_pending_video_drop_actions":
    case "generate_thumbnails":
    case "generate_timeline_thumbnails":
      return [];
    case "get_default_export_dir":
    case "pick_export_folder":
      return "C:\\Users\\user\\Downloads";
    case "get_media_server_port":
      return 1420;
    case "is_maximized":
      return false;
    case "get_export_capabilities":
      return { available: true, ffmpegAvailable: true };
    case "check_bg_downloaded":
      return { downloaded: false, ext: null, version: 0 };
    case "get_bg_download_progress":
      return null;
    case "prewarm_custom_background":
    case "start_bg_download":
    case "delete_bg_download":
      return null;
    case "get_subtitle_generation_capabilities":
      return { methods: [], available: false, reason: "test harness" };
    case "get_subtitle_translation_capabilities":
      return { methods: [], available: false, reason: "test harness" };
    case "get_narration_tts_metadata":
      return {
        providers: [
          { method: "GeminiLive", label: "Gemini Live" },
          { method: "EdgeTTS", label: "Edge TTS" },
          { method: "GoogleTranslate", label: "Google Translate" },
          { method: "Kokoro", label: "Kokoro 82M v1.0" },
          { method: "Supertonic", label: "Supertonic 3" },
          { method: "StepAudioEditX", label: "Step Audio EditX" },
          { method: "MagpieMultilingual", label: "Magpie Multilingual" },
        ],
        geminiVoices: [],
        geminiModels: [],
        geminiInstructionLanguages: [],
        geminiSpeedOptions: ["Slow", "Normal", "Fast"],
        googleSpeedOptions: ["Slow", "Normal"],
        kokoroVoiceLanguages: [
          { languageCode: "eng", languageName: "English" },
          { languageCode: "jpn", languageName: "Japanese" },
        ],
        kokoroVoices: [
          { id: "af_heart", label: "Heart", languageCode: "en-us" },
          { id: "jf_alpha", label: "Alpha", languageCode: "ja" },
        ],
        supertonicLanguages: [
          { languageCode: "en", languageName: "English" },
          { languageCode: "vi", languageName: "Vietnamese" },
        ],
        supertonicVoices: [
          { id: "F1", label: "F1" },
          { id: "M1", label: "M1" },
        ],
        stepAudioVoices: [
          { id: "ref-demo", label: "Demo reference" },
        ],
        stepAudioReferenceVoices: [
          { id: "ref-demo", label: "Demo reference", audioPath: "", transcript: "" },
        ],
        edgeVoiceState: "loaded",
        edgeVoiceLanguages: [],
        edgeVoicesByLanguage: {},
        defaults: {
          method: "GeminiLive",
          geminiModel: "",
          geminiVoice: "",
          geminiSpeed: "Normal",
          geminiInstruction: "",
          geminiLanguageConditions: [],
          googleSpeed: "Normal",
          edgeVoice: "",
          edgePitch: 0,
          edgeRate: 0,
          edgeVoiceConfigs: [],
          kokoroVoice: "af_heart",
          kokoroSpeed: 1,
          kokoroNumThreads: 2,
          kokoroVoiceConfigs: [
            { languageCode: "eng", languageName: "English", voiceId: "af_heart" },
            { languageCode: "jpn", languageName: "Japanese", voiceId: "jf_alpha" },
          ],
          stepAudioVoice: "default_en",
          stepAudioReferenceVoiceId: "",
          stepAudioPromptText: "",
          stepAudioUseCustomReference: false,
          stepAudioReferenceAudioPath: "",
          stepAudioReferenceText: "",
          stepAudioReferenceLabel: "",
          magpieVoice: "",
          magpieVoiceConfigs: [],
          supertonicSpeed: 1,
          supertonicNumSteps: 5,
          supertonicNumThreads: 2,
          supertonicVoiceConfigs: [
            { languageCode: "en", languageName: "English", voiceId: "M1" },
            { languageCode: "vi", languageName: "Vietnamese", voiceId: "F1" },
          ],
        },
      };
    case "probe_video_metadata":
      return { width: 1920, height: 1080, fps: 60, duration: 600 };
    case "import_video_path":
      return { path: String(args?.path ?? "C:\\SGT-Test\\imported.mp4"), hasAudio: true };
    case "import_audio_path":
      return { path: String(args?.path ?? "C:\\SGT-Test\\imported.wav"), duration: 60 };
    case "create_audio_placeholder_video":
      return { path: "C:\\SGT-Test\\placeholder.mp4" };
    case "start_audio_download":
      return {
        status: "success",
        path: "C:\\Users\\user\\Downloads\\SGT_Test_Audio.wav",
        format: args?.format ?? "wav",
        trackKind: args?.trackKind ?? "imported",
      };
    case "start_export_server":
      return { status: "success", path: "C:\\Users\\user\\Downloads\\SGT_Test_Export.mp4" };
    default:
      return null;
  }
}

export function installBrowserTestIpcMock() {
  if (!isScreenRecordTestHarnessEnabled()) return;
  const testWindow = window as TestWindow;
  if (testWindow.isWry || typeof testWindow.invoke === "function") return;
  const calls: IpcCall[] = [];
  testWindow.__SGT_TEST_IPC_CALLS__ = calls;
  testWindow.invoke = async <T,>(cmd: string, args?: Record<string, unknown>): Promise<T> => {
    calls.push({ cmd, args, at: performance.now() });
    return defaultResponse(cmd, args) as T;
  };
  testWindow.ipc = {
    postMessage: (message: string) => {
      calls.push({ cmd: "ipc.postMessage", args: { message }, at: performance.now() });
    },
  };
}
