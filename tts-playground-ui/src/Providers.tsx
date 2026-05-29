import { useTtsState } from "./state";
import { ttsApi } from "./ipc";
import { AudioEditPanel, S2SPanel } from "./Modes";
import { ReferenceLibraryPanel } from "./ReferenceLibrary";
import {
  Card,
  FormRow,
  NumberRange,
  Select,
  SmallButton,
  SpeedRadios,
} from "./components";
import type {
  CatalogOption,
  EdgeVoiceConfig,
  LocalVoiceConfig,
  TtsMethod,
  TtsMode,
} from "./types";

/** Top-level switcher — picks the right panel by mode/method. */
export function ProviderPanel({ forceMode }: { forceMode?: TtsMode } = {}) {
  const s = useTtsState();
  const mode = forceMode ?? s.mode;
  if (mode === "AudioEdit") return <AudioEditPanel />;
  if (mode === "SpeechToSpeech") return <S2SPanel />;
  if (mode === "ReferenceLibrary") return <ReferenceLibraryPanel />;
  switch (s.method as TtsMethod) {
    case "GeminiLive":
      return <GeminiPanel />;
    case "EdgeTTS":
      return <EdgePanel />;
    case "GoogleTranslate":
      return <GooglePanel />;
    case "StepAudioEditX":
      return <StepAudioPanel />;
    case "MagpieMultilingual":
      return <LocalProviderPanelView provider="magpie" />;
    case "Kokoro":
      return <LocalProviderPanelView provider="kokoro" />;
    case "Supertonic":
      return <LocalProviderPanelView provider="supertonic" />;
    case "VieneuTts":
      return <VieneuPanel />;
  }
}

// ----------------------------------------------------------------------------
// Per-provider panels
// ----------------------------------------------------------------------------

function GeminiPanel() {
  const s = useTtsState();
  const g = s.gemini;
  return (
    <Card title={s.strings.methodGemini} className="tts-panel-gemini">
      <FormRow label={s.strings.geminiModelLabel}>
        <Select
          value={g.model}
          options={s.catalogs.geminiModels}
          onChange={(model) => void ttsApi.patchGemini({ model })}
        />
      </FormRow>
      <FormRow label={s.strings.referenceVoice}>
        <div className="grid grid-cols-[1fr,auto] gap-2">
          <Select
            value={g.voice}
            options={s.catalogs.geminiVoices.map((v) => ({
              value: v.value,
              label: `${v.label} (${v.gender})`,
            }))}
            onChange={(voice) => void ttsApi.patchGemini({ voice })}
          />
          <SmallButton onClick={() => void ttsApi.previewVoice(g.voice)}>
            {s.strings.preview}
          </SmallButton>
        </div>
      </FormRow>
      <FormRow label={s.strings.speedLabel}>
        <SpeedRadios
          value={g.speed}
          onChange={(speed) => void ttsApi.patchGemini({ speed })}
          strings={s.strings}
        />
      </FormRow>
      <div className="tts-gemini-instruction-field flex flex-col gap-1.5">
        <label className="text-xs font-medium text-muted">
          {s.strings.instructionsLabel}
        </label>
        <textarea
          value={g.instruction}
          placeholder={s.strings.instructionsHint}
          onChange={(ev) =>
            void ttsApi.patchGemini({ instruction: ev.target.value })
          }
          className="tts-gemini-instruction min-h-[52px] w-full resize-none rounded-md bg-surface px-2.5 py-2 text-xs leading-relaxed text-fg outline-none transition focus:ring-2 focus:ring-accent/25"
        />
      </div>
      <FormRow label={s.strings.voicePerLanguage}>
        <div className="tts-gemini-conditions flex flex-col gap-1.5">
          {g.conditions.map((c, idx) => (
            <div
              key={`${c.language}-${idx}`}
              className="tts-gemini-condition-row grid grid-cols-[112px,1fr,24px] items-center gap-2 rounded-md bg-surface px-2 py-1.5"
            >
              <span className="tts-condition-lang truncate text-xs font-medium text-muted">
                {c.name || c.language}
              </span>
              <input
                value={c.instruction}
                onChange={(ev) => {
                  const next = g.conditions.map((item, i) =>
                    i === idx
                      ? { ...item, instruction: ev.target.value }
                      : item,
                  );
                  void ttsApi.patchGemini({ conditions: next });
                }}
                className="min-w-0 rounded-md bg-surface-soft px-2 py-1 text-xs text-fg outline-none transition focus:ring-2 focus:ring-accent/25"
              />
              <button
                onClick={() =>
                  void ttsApi.patchGemini({
                    conditions: g.conditions.filter((_, i) => i !== idx),
                  })
                }
                className="tts-condition-remove text-muted hover:text-danger"
                aria-label="Remove"
              >
                ×
              </button>
            </div>
          ))}
          <Select
            value=""
            placeholder={s.strings.addLanguage + "…"}
            options={s.catalogs.geminiInstructionLanguages.filter(
              (lang) => !g.conditions.some((c) => c.language === lang.value),
            )}
            onChange={(language) => {
              if (!language) return;
              const found = s.catalogs.geminiInstructionLanguages.find(
                (lang) => lang.value === language,
              );
              void ttsApi.patchGemini({
                conditions: [
                  ...g.conditions,
                  {
                    language,
                    name: found?.label ?? language,
                    instruction: "",
                  },
                ],
              });
            }}
          />
        </div>
      </FormRow>
    </Card>
  );
}

function EdgePanel() {
  const s = useTtsState();
  const e = s.edge;
  return (
    <Card
      title={s.strings.methodEdge}
      className="tts-panel-edge"
      action={
        <SmallButton onClick={() => void ttsApi.resetProvider("edge")}>
          {s.strings.reset}
        </SmallButton>
      }
    >
      <FormRow label={s.strings.pitchLabel}>
        <NumberRange
          value={e.pitch}
          min={-50}
          max={50}
          step={1}
          suffix=" Hz"
          onChange={(pitch) => void ttsApi.patchEdge({ pitch })}
        />
      </FormRow>
      <FormRow label={s.strings.rateLabel}>
        <NumberRange
          value={e.rate}
          min={-50}
          max={100}
          step={1}
          suffix="%"
          onChange={(rate) => void ttsApi.patchEdge({ rate })}
        />
      </FormRow>
      <VoicePerLanguageEdge voices={e.voices} />
    </Card>
  );
}

function VoicePerLanguageEdge({ voices }: { voices: EdgeVoiceConfig[] }) {
  const s = useTtsState();
  return (
    <div className="tts-voice-grid tts-voice-grid-edge flex flex-col gap-1.5">
      <div className="tts-voice-grid-title text-xs font-medium uppercase tracking-wide text-muted">
        {s.strings.voicePerLanguage}
      </div>
      <ul className="tts-voice-list flex max-h-44 flex-col gap-1 overflow-y-auto">
        {voices.map((v) => {
          const opts =
            s.catalogs.edgeVoicesByLanguage[v.language] ??
            ([] as CatalogOption[]);
          return (
            <li
              key={v.language}
              className="tts-voice-row grid grid-cols-[72px,1fr,auto,24px] items-center gap-2 rounded-md bg-surface px-2 py-1.5"
            >
              <span className="tts-voice-lang font-mono text-xs text-muted">
                {v.language}
              </span>
              <Select
                value={v.voice}
                options={opts}
                onChange={(voice) => {
                  const next = voices.map((existing) =>
                    existing.language === v.language
                      ? { ...existing, voice }
                      : existing,
                  );
                  void ttsApi.patchEdge({ voices: next });
                }}
              />
              <SmallButton onClick={() => void ttsApi.previewVoice(v.voice)}>
                {s.strings.preview}
              </SmallButton>
              <button
                onClick={() => {
                  const next = voices.filter((x) => x.language !== v.language);
                  void ttsApi.patchEdge({ voices: next });
                }}
                className="tts-voice-remove text-muted hover:text-danger"
                aria-label="Remove"
              >
                <svg viewBox="0 0 16 16" className="h-3.5 w-3.5">
                  <path
                    d="M5 5 L11 11 M11 5 L5 11"
                    stroke="currentColor"
                    strokeWidth="1.5"
                    strokeLinecap="round"
                  />
                </svg>
              </button>
            </li>
          );
        })}
      </ul>
      <AddLanguageRow
        languages={s.catalogs.edgeAvailableLanguages.filter(
          (l) => !voices.some((v) => v.language === l.value),
        )}
        onAdd={(language) => {
          const opts = s.catalogs.edgeVoicesByLanguage[language] ?? [];
          const voice = opts[0]?.value ?? "";
          void ttsApi.patchEdge({
            voices: [...voices, { language, voice }],
          });
        }}
      />
    </div>
  );
}

function GooglePanel() {
  const s = useTtsState();
  return (
    <Card title={s.strings.methodGoogle} className="tts-panel-google">
      <FormRow label={s.strings.speedLabel}>
        <SpeedRadios
          value={s.google.speed}
          onChange={(speed) => void ttsApi.patchGoogle({ speed })}
          strings={s.strings}
        />
      </FormRow>
    </Card>
  );
}

function StepAudioPanel() {
  const s = useTtsState();
  const refs = s.catalogs.stepAudioReferences.map((r) => ({
    value: r.id,
    label: r.name,
  }));
  return (
    <Card
      title={s.strings.methodStepAudio}
      className="tts-panel-step-audio"
      description={s.strings.stepAudioDesc}
      action={
        <SmallButton onClick={() => void ttsApi.resetProvider("stepAudio")}>
          {s.strings.reset}
        </SmallButton>
      }
    >
      <FormRow label={s.strings.referenceVoice}>
        <Select
          value={s.stepAudio.reference}
          options={refs}
          onChange={(reference) => void ttsApi.patchStepAudio({ reference })}
          placeholder="—"
        />
      </FormRow>
    </Card>
  );
}

function LocalProviderPanelView({
  provider,
}: {
  provider: "magpie" | "kokoro" | "supertonic";
}) {
  const s = useTtsState();
  const settings =
    provider === "magpie"
      ? s.magpie
      : provider === "kokoro"
      ? s.kokoro
      : s.supertonic;
  const title =
    provider === "magpie"
      ? s.strings.methodMagpie
      : provider === "kokoro"
      ? s.strings.methodKokoro
      : s.strings.methodSupertonic;
  const voicesByLang =
    provider === "magpie"
      ? s.catalogs.magpieVoicesByLanguage
      : provider === "kokoro"
      ? s.catalogs.kokoroVoicesByLanguage
      : s.catalogs.supertonicVoicesByLanguage;
  const availableLangs =
    provider === "magpie"
      ? s.catalogs.magpieAvailableLanguages
      : provider === "kokoro"
      ? s.catalogs.kokoroAvailableLanguages
      : s.catalogs.supertonicAvailableLanguages;
  const patch =
    provider === "magpie"
      ? ttsApi.patchMagpie
      : provider === "kokoro"
      ? ttsApi.patchKokoro
      : ttsApi.patchSupertonic;
  // Magpie backend (`MagpieSettings`) doesn't carry speed/threads knobs, so
  // those rows only render for Kokoro + Supertonic where the backend honors
  // them.
  const showRuntimeKnobs = provider !== "magpie";
  return (
    <Card
      title={title}
      className={`tts-panel-${provider}`}
      action={
        <SmallButton onClick={() => void ttsApi.resetProvider(provider)}>
          {s.strings.reset}
        </SmallButton>
      }
    >
      {showRuntimeKnobs && (
        <FormRow label={s.strings.speedLabel}>
          <NumberRange
            value={settings.speed}
            min={0.5}
            max={2}
            step={0.05}
            onChange={(speed) => void patch({ speed })}
          />
        </FormRow>
      )}
      {showRuntimeKnobs && (
        <FormRow label={s.strings.threadsLabel}>
          <NumberRange
            value={settings.threads}
            min={1}
            max={8}
            step={1}
            onChange={(threads) => void patch({ threads })}
          />
        </FormRow>
      )}
      {provider === "supertonic" && "steps" in settings && (
        <FormRow label={s.strings.qualityStepsLabel}>
          <NumberRange
            value={settings.steps ?? 24}
            min={1}
            max={20}
            step={1}
            onChange={(steps) => void ttsApi.patchSupertonic({ steps })}
          />
        </FormRow>
      )}
      <VoicePerLanguageLocal
        voices={settings.voices}
        voicesByLanguage={voicesByLang}
        availableLanguages={availableLangs}
        onChange={(voices) => void patch({ voices })}
      />
    </Card>
  );
}

function VoicePerLanguageLocal({
  voices,
  voicesByLanguage,
  availableLanguages,
  onChange,
}: {
  voices: LocalVoiceConfig[];
  voicesByLanguage: Record<string, CatalogOption[]>;
  availableLanguages: CatalogOption[];
  onChange: (next: LocalVoiceConfig[]) => void;
}) {
  const s = useTtsState();
  return (
    <div className="tts-voice-grid tts-voice-grid-local flex flex-col gap-1.5">
      <div className="tts-voice-grid-title text-xs font-medium uppercase tracking-wide text-muted">
        {s.strings.voicePerLanguage}
      </div>
      <ul className="tts-voice-list flex max-h-44 flex-col gap-1 overflow-y-auto">
        {voices.map((v) => {
          const opts = voicesByLanguage[v.language] ?? [];
          return (
            <li
              key={v.language}
              className="tts-voice-row grid grid-cols-[72px,1fr,auto,24px] items-center gap-2 rounded-md bg-surface px-2 py-1.5"
            >
              <span className="tts-voice-lang font-mono text-xs text-muted">
                {v.language}
              </span>
              <Select
                value={v.voice}
                options={opts}
                onChange={(voice) =>
                  onChange(
                    voices.map((x) =>
                      x.language === v.language ? { ...x, voice } : x,
                    ),
                  )
                }
              />
              <SmallButton onClick={() => void ttsApi.previewVoice(v.voice)}>
                {s.strings.preview}
              </SmallButton>
              <button
                onClick={() =>
                  onChange(voices.filter((x) => x.language !== v.language))
                }
                className="tts-voice-remove text-muted hover:text-danger"
                aria-label="Remove"
              >
                <svg viewBox="0 0 16 16" className="h-3.5 w-3.5">
                  <path
                    d="M5 5 L11 11 M11 5 L5 11"
                    stroke="currentColor"
                    strokeWidth="1.5"
                    strokeLinecap="round"
                  />
                </svg>
              </button>
            </li>
          );
        })}
      </ul>
      <AddLanguageRow
        languages={availableLanguages.filter(
          (l) => !voices.some((v) => v.language === l.value),
        )}
        onAdd={(language) => {
          const opts = voicesByLanguage[language] ?? [];
          onChange([...voices, { language, voice: opts[0]?.value ?? "" }]);
        }}
      />
    </div>
  );
}

function AddLanguageRow({
  languages,
  onAdd,
}: {
  languages: CatalogOption[];
  onAdd: (lang: string) => void;
}) {
  const s = useTtsState();
  if (languages.length === 0) return null;
  return (
    <div className="tts-add-language grid grid-cols-[1fr,auto] gap-2">
      <Select
        value=""
        placeholder={s.strings.addLanguage + "…"}
        options={languages}
        onChange={(language) => {
          if (language) onAdd(language);
        }}
      />
    </div>
  );
}

function VieneuPanel() {
  const s = useTtsState();
  const refs = s.catalogs.vieneuReferences.map((r) => ({
    value: r.id,
    label: r.name,
  }));
  return (
    <Card
      title={s.strings.methodVieneu}
      className="tts-panel-vieneu"
      description={s.strings.vieneuDesc}
      action={
        <SmallButton onClick={() => void ttsApi.resetProvider("vieneu")}>
          {s.strings.reset}
        </SmallButton>
      }
    >
      <FormRow label={s.strings.referenceVoice}>
        <Select
          value={s.vieneu.reference}
          options={refs}
          onChange={(reference) => void ttsApi.patchVieneu({ reference })}
          placeholder="—"
        />
      </FormRow>
    </Card>
  );
}
