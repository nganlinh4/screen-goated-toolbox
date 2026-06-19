import { useEffect } from "react";
import clsx from "clsx";
import { useThemeAttr, useTtsState } from "./state";
import { ttsApi } from "./ipc";
import type { TtsMethod, TtsMode } from "./types";
import { ProviderPanel } from "./Providers";
import { Studio } from "./Studio";
import { Select } from "./components";

export function App() {
  useThemeAttr();
  const s = useTtsState();

  // Forward all keyboard events to the WebView so the WRY window keeps focus
  // and ESC etc. don't accidentally hit the host app underneath.
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        void ttsApi.closeWindow();
      }
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, []);

  return (
    <div className="tts-root flex h-full flex-col bg-surface text-fg">
      <Header title={s.strings.title} />
      <ModeBar
        mode={s.mode}
        strings={{
          ttsClone: s.strings.modeTtsClone,
          audioEdit: s.strings.modeAudioEdit,
          referenceLibrary: s.strings.modeReferenceLibrary,
          s2s: s.strings.modeS2S,
        }}
      />
      <div className="tts-body flex min-h-0 flex-1 gap-4 px-4 pb-4 pt-3">
        <section className="tts-controls flex min-w-0 flex-1 flex-col gap-3 overflow-y-auto overflow-x-hidden pr-1">
          {s.mode === "TtsClone" && (
            <>
              <MethodPicker method={s.method} strings={s.strings} />
              <ProviderPanel />
            </>
          )}
          {s.mode === "AudioEdit" && <ProviderPanel forceMode="AudioEdit" />}
          {s.mode === "SpeechToSpeech" && (
            <ProviderPanel forceMode="SpeechToSpeech" />
          )}
          {s.mode === "ReferenceLibrary" && (
            <ProviderPanel forceMode="ReferenceLibrary" />
          )}
        </section>
        <aside className="tts-studio flex shrink-0 basis-[min(42%,360px)] min-w-[280px] flex-col">
          <Studio />
        </aside>
      </div>
    </div>
  );
}

function Header({ title }: { title: string }) {
  return (
    <header
      className="tts-header flex h-11 items-center justify-between bg-surface-container px-4"
      onMouseDown={(e) => {
        // Left-button only, and never when the press lands on an interactive
        // control (the window buttons) — otherwise the OS drag loop swallows
        // their click. Mirrors Translation Gummy's drag guard.
        if (e.button !== 0) return;
        if ((e.target as HTMLElement).closest("button, input, textarea, a, [role='button']")) {
          return;
        }
        void ttsApi.startDrag();
      }}
      onDoubleClick={() => void ttsApi.minimizeWindow()}
    >
      <span className="tts-title flex items-center gap-2.5 text-sm font-semibold tracking-tight">
        <WaveMark />
        {title}
      </span>
      <div className="tts-window-controls no-drag flex items-center gap-1">
        <WindowButton
          ariaLabel="Minimize"
          onClick={() => void ttsApi.minimizeWindow()}
        >
          <svg viewBox="0 0 24 24" className="h-3 w-3" fill="currentColor">
            <path d="M5 13v-2h14v2z" />
          </svg>
        </WindowButton>
        <WindowButton
          ariaLabel="Close"
          onClick={() => void ttsApi.closeWindow()}
          danger
        >
          <svg viewBox="0 0 24 24" className="h-3 w-3" fill="currentColor">
            <path d="M6.4 19L5 17.6l5.6-5.6L5 6.4L6.4 5l5.6 5.6L17.6 5L19 6.4L13.4 12l5.6 5.6l-1.4 1.4l-5.6-5.6z" />
          </svg>
        </WindowButton>
      </div>
    </header>
  );
}

/** Small accent waveform mark (Material Symbols "graphic_eq") — replaces the generic 🔊 emoji. */
function WaveMark() {
  return (
    <svg viewBox="0 0 24 24" className="tts-wave-mark h-3.5 w-4 text-accent" fill="currentColor" aria-hidden>
      <path d="M7 18V6h2v12zm4 4V2h2v20zm-8-8v-4h2v4zm12 4V6h2v12zm4-4v-4h2v4z" />
    </svg>
  );
}

function WindowButton({
  children,
  onClick,
  ariaLabel,
  danger,
}: {
  children: React.ReactNode;
  onClick: () => void;
  ariaLabel: string;
  danger?: boolean;
}) {
  return (
    <button
      aria-label={ariaLabel}
      onClick={onClick}
      className={clsx(
        "tts-window-btn no-drag flex h-7 w-7 items-center justify-center rounded-md text-muted transition ease-spring",
        danger
          ? "tts-window-btn-close hover:bg-danger/15 hover:text-danger"
          : "tts-window-btn-minimize hover:bg-surface-strong hover:text-fg",
      )}
    >
      {children}
    </button>
  );
}

function ModeBar({
  mode,
  strings,
}: {
  mode: TtsMode;
  strings: {
    ttsClone: string;
    audioEdit: string;
    referenceLibrary: string;
    s2s: string;
  };
}) {
  const tabs: Array<{ id: TtsMode; label: string }> = [
    { id: "SpeechToSpeech", label: strings.s2s },
    { id: "TtsClone", label: strings.ttsClone },
    { id: "AudioEdit", label: strings.audioEdit },
    { id: "ReferenceLibrary", label: strings.referenceLibrary },
  ];
  return (
    <nav className="tts-mode-bar bg-surface-container px-4 pb-2.5 pt-1">
      <div className="tts-mode-track inline-flex gap-0.5 rounded-md bg-surface p-0.5">
        {tabs.map((t) => {
          const active = mode === t.id;
          return (
            <button
              key={t.id}
              onClick={() => void ttsApi.setMode(t.id)}
              className={clsx(
                `tts-mode-tab tts-mode-tab-${t.id}`,
                "rounded-[6px] px-3 py-1 text-xs font-medium transition ease-spring",
                active
                  ? "tts-mode-tab--active bg-surface-soft text-fg shadow-elevation-1"
                  : "text-muted hover:text-fg",
              )}
            >
              {t.label}
            </button>
          );
        })}
      </div>
    </nav>
  );
}

function MethodPicker({
  method,
  strings,
}: {
  method: TtsMethod;
  strings: ReturnType<typeof useTtsState>["strings"];
}) {
  const methods: Array<{ id: TtsMethod; label: string }> = [
    { id: "GeminiLive", label: strings.methodGemini },
    { id: "EdgeTTS", label: strings.methodEdge },
    { id: "GoogleTranslate", label: strings.methodGoogle },
    { id: "StepAudioEditX", label: strings.methodStepAudio },
    { id: "MagpieMultilingual", label: strings.methodMagpie },
    { id: "Kokoro", label: strings.methodKokoro },
    { id: "Supertonic", label: strings.methodSupertonic },
    { id: "VieneuTts", label: strings.methodVieneu },
  ];
  return (
    <div className="tts-method-picker flex items-center gap-3 rounded-lg bg-surface-soft px-3.5 py-2.5 shadow-elevation-2">
      <label className="tts-method-label shrink-0 text-xs font-medium uppercase tracking-wide text-muted">
        {strings.methodLabel}
      </label>
      <div className="min-w-0 flex-1">
        <Select
          value={method}
          options={methods.map((m) => ({ value: m.id, label: m.label }))}
          onChange={(id) => void ttsApi.setMethod(id)}
          className="tts-method-select"
        />
      </div>
    </div>
  );
}
