import { useEffect } from "react";
import clsx from "clsx";
import { useThemeAttr, useTtsState } from "./state";
import { ttsApi } from "./ipc";
import type { TtsMethod, TtsMode } from "./types";
import { ProviderPanel } from "./Providers";
import { Studio } from "./Studio";

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
      <div className="tts-body flex min-h-0 flex-1 gap-3 px-4 pb-4">
        <section className="tts-controls flex min-w-0 flex-1 flex-col overflow-y-auto pr-1">
          {s.mode === "TtsClone" && (
            <>
              <MethodPicker method={s.method} strings={s.strings} />
              <div className="h-2" />
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
        <div className="w-px bg-border/60" />
        <aside className="tts-studio flex w-[300px] shrink-0 flex-col">
          <Studio />
        </aside>
      </div>
    </div>
  );
}

function Header({ title }: { title: string }) {
  return (
    <header
      className="drag-region flex h-10 items-center justify-between border-b border-border/60 bg-surface-soft px-4"
      onPointerDown={(e) => {
        // Only left-button drags; right click should not move the window.
        if (e.button === 0) {
          void ttsApi.startDrag();
        }
      }}
      onDoubleClick={() => void ttsApi.minimizeWindow()}
    >
      <span className="text-sm font-medium tracking-tight">
        <span className="mr-2 text-accent">🔊</span>
        {title}
      </span>
      <div className="no-drag flex items-center gap-1">
        <WindowButton
          ariaLabel="Minimize"
          onClick={() => void ttsApi.minimizeWindow()}
        >
          <svg viewBox="0 0 12 12" className="h-3 w-3">
            <rect x="2" y="5.5" width="8" height="1" fill="currentColor" />
          </svg>
        </WindowButton>
        <WindowButton
          ariaLabel="Close"
          onClick={() => void ttsApi.closeWindow()}
          danger
        >
          <svg viewBox="0 0 12 12" className="h-3 w-3">
            <path
              d="M2 2 L10 10 M10 2 L2 10"
              stroke="currentColor"
              strokeWidth="1.4"
              strokeLinecap="round"
            />
          </svg>
        </WindowButton>
      </div>
    </header>
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
        "no-drag flex h-7 w-7 items-center justify-center rounded-md text-muted transition-colors",
        danger
          ? "hover:bg-danger/15 hover:text-danger"
          : "hover:bg-surface-strong hover:text-fg",
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
    <nav className="border-b border-border/60 bg-surface-soft px-3">
      <div className="flex items-end gap-1">
        {tabs.map((t) => {
          const active = mode === t.id;
          return (
            <button
              key={t.id}
              onClick={() => void ttsApi.setMode(t.id)}
              className={clsx(
                "relative -mb-px px-3 py-2 text-xs font-medium transition-colors",
                active
                  ? "text-fg"
                  : "text-muted hover:text-fg",
              )}
            >
              {t.label}
              {active && (
                <span className="absolute inset-x-2 -bottom-px h-[2px] rounded-full bg-accent" />
              )}
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
    <div className="card flex items-center gap-3 rounded-lg border border-border bg-surface-soft px-3 py-2 shadow-sm">
      <label className="text-xs font-medium text-muted">
        {strings.methodLabel}
      </label>
      <select
        value={method}
        onChange={(e) => void ttsApi.setMethod(e.target.value as TtsMethod)}
        className="flex-1 rounded-md border border-border bg-surface px-2 py-1 text-sm focus:border-accent focus:outline-none"
      >
        {methods.map((m) => (
          <option key={m.id} value={m.id}>
            {m.label}
          </option>
        ))}
      </select>
    </div>
  );
}
