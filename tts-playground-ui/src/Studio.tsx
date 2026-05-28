import clsx from "clsx";
import { useTtsState } from "./state";
import { ttsApi } from "./ipc";

export function Studio() {
  const s = useTtsState();
  return (
    <div className="flex h-full flex-col gap-3">
      {s.mode === "SpeechToSpeech" ? <SpeechToSpeechInput /> : <TextInput />}
      <Player />
      <RecentClips />
      <Exports />
      {s.player.error && (
        <div className="rounded-md bg-danger/10 px-3 py-2 text-xs text-danger">
          {s.player.error}
        </div>
      )}
    </div>
  );
}

function SpeechToSpeechInput() {
  const s = useTtsState();
  return (
    <div className="flex flex-col gap-2 rounded-lg border border-border bg-surface-soft p-3 shadow-sm">
      <label className="text-xs font-medium text-muted">Gemini S2S</label>
      <p className="truncate text-[11px] text-muted" title={s.audioEdit.sourcePath}>
        {s.audioEdit.sourcePath || s.strings.noSource}
      </p>
      <div className="flex items-center justify-end gap-1.5">
        <button
          onClick={() => void ttsApi.generate()}
          disabled={s.player.isGenerating || !s.audioEdit.sourcePath}
          className={clsx(
            "rounded-md px-3 py-1 text-xs font-medium shadow-sm transition-colors",
            s.player.isGenerating || !s.audioEdit.sourcePath
              ? "cursor-not-allowed bg-accent-soft text-muted"
              : "bg-accent text-white hover:brightness-110",
          )}
        >
          {s.player.isGenerating ? s.strings.generating : s.strings.generate}
        </button>
        {s.player.isGenerating && (
          <button
            onClick={() => void ttsApi.cancelGeneration()}
            className="text-[11px] text-muted underline-offset-2 hover:text-fg hover:underline"
          >
            {s.strings.cancel}
          </button>
        )}
      </div>
    </div>
  );
}

function TextInput() {
  const s = useTtsState();
  const editMode = s.mode === "AudioEdit" && s.audioEdit.editType !== "paralinguistic";
  const canGenerate =
    !s.player.isGenerating &&
    (s.mode === "AudioEdit"
      ? Boolean(
          s.audioEdit.sourcePath.trim() &&
            s.audioEdit.sourceText.trim() &&
            (s.audioEdit.editType !== "paralinguistic" ||
              s.audioEdit.targetText.trim()),
        )
      : Boolean(s.draftText.trim()));
  // For audio-edit non-paralinguistic flows the source transcript IS the input;
  // for everything else the draft text is the synth input.
  return (
    <div className="flex flex-col gap-2 rounded-lg border border-border bg-surface-soft p-3 shadow-sm">
      <label className="text-xs font-medium text-muted">
        {editMode ? s.strings.sourceTranscript : s.strings.textLabel}
      </label>
      <textarea
        value={editMode ? s.audioEdit.sourceText : s.draftText}
        onChange={(e) => {
          if (editMode) {
            void ttsApi.patchAudioEdit({ sourceText: e.target.value });
          } else {
            void ttsApi.setDraftText(e.target.value);
          }
        }}
        placeholder={s.strings.textHint}
        className="min-h-[120px] resize-none rounded-md border border-border bg-surface px-2 py-1.5 font-mono text-[12px] leading-relaxed text-fg outline-none focus:border-accent"
      />
      <div className="flex items-center justify-between">
        <span className="text-[11px] text-muted">
          {(s.strings.charCountTemplate || "{n} chars").replace(
            "{n}",
            String((editMode ? s.audioEdit.sourceText : s.draftText).length),
          )}
        </span>
        <div className="flex items-center gap-1.5">
          <button
            onClick={() => void ttsApi.clear()}
            className="rounded-md border border-border bg-surface px-2.5 py-1 text-xs text-muted hover:border-border-strong hover:text-fg"
          >
            {s.strings.clear}
          </button>
          <button
            onClick={() => void ttsApi.generate()}
            disabled={!canGenerate}
            className={clsx(
              "rounded-md px-3 py-1 text-xs font-medium shadow-sm transition-colors",
              !canGenerate
                ? "cursor-not-allowed bg-accent-soft text-muted"
                : "bg-accent text-white hover:brightness-110",
            )}
          >
            {s.player.isGenerating
              ? s.strings.generating
              : s.strings.generate}
          </button>
        </div>
      </div>
      {s.player.isGenerating && (
        <button
          onClick={() => void ttsApi.cancelGeneration()}
          className="self-end text-[11px] text-muted underline-offset-2 hover:text-fg hover:underline"
        >
          {s.strings.cancel}
        </button>
      )}
    </div>
  );
}

function Player() {
  const s = useTtsState();
  const current = s.player.current;
  if (!current) {
    return (
      <div className="rounded-lg border border-border bg-surface-soft px-3 py-3 text-center text-xs text-muted shadow-sm">
        {s.strings.noAudio}
      </div>
    );
  }
  const duration = Math.max(0.001, current.durationSec);
  const pct = Math.min(100, (s.player.positionSec / duration) * 100);
  const playLabel = s.player.isPlaying
    ? s.strings.pause
    : s.player.paused
    ? s.strings.resume
    : s.strings.play;
  return (
    <div className="flex flex-col gap-2 rounded-lg border border-border bg-surface-soft p-3 shadow-sm">
      <div className="flex items-baseline justify-between gap-2">
        <span className="truncate text-xs font-medium" title={current.voiceLabel}>
          {current.voiceLabel}
        </span>
        <span className="font-mono text-[11px] text-muted">
          {fmt(s.player.positionSec)} / {fmt(duration)}
        </span>
      </div>
      <input
        type="range"
        min={0}
        max={duration}
        step={0.05}
        value={s.player.positionSec}
        onChange={(e) => void ttsApi.seek(Number(e.target.value))}
        className="seek-bar"
        style={{
          background: `linear-gradient(to right, rgb(var(--accent)) ${pct}%, rgb(var(--border-strong)) ${pct}%)`,
        }}
      />
      <div className="flex items-center gap-1.5">
        <button
          onClick={() => {
            if (s.player.isPlaying) void ttsApi.pause();
            else void ttsApi.play();
          }}
          className="rounded-md bg-accent px-3 py-1 text-xs font-medium text-white hover:brightness-110"
        >
          {playLabel}
        </button>
        <button
          onClick={() => void ttsApi.stop()}
          className="rounded-md border border-border bg-surface px-2.5 py-1 text-xs text-fg hover:border-border-strong"
        >
          {s.strings.stop}
        </button>
        <button
          onClick={() => void ttsApi.replay()}
          className="rounded-md border border-border bg-surface px-2.5 py-1 text-xs text-fg hover:border-border-strong"
        >
          {s.strings.replay}
        </button>
      </div>
    </div>
  );
}

function RecentClips() {
  const s = useTtsState();
  if (s.player.recent.length === 0) return null;
  return (
    <div className="flex min-h-0 flex-1 flex-col gap-1.5 rounded-lg border border-border bg-surface-soft p-3 shadow-sm">
      <div className="text-xs font-medium text-muted">{s.strings.recent}</div>
      <ul className="flex flex-col gap-0.5 overflow-y-auto">
        {s.player.recent.map((clip) => (
          <li
            key={clip.id}
            className="group flex cursor-pointer items-center justify-between gap-2 rounded px-2 py-1 text-[11px] hover:bg-surface-strong"
            onClick={() => void ttsApi.playRecent(clip.id)}
          >
            <span className="flex min-w-0 flex-1 items-center gap-1.5 truncate">
              <span className="shrink-0 font-mono text-muted">
                {clip.createdLabel}
              </span>
              <span className="truncate text-muted">|</span>
              <span className="shrink-0 truncate font-medium" title={clip.voiceLabel}>
                {clip.voiceLabel}
              </span>
              <span className="shrink-0 font-mono text-muted">
                {clip.durationSec.toFixed(1)}s
              </span>
              <span className="truncate text-muted">
                {flatten(clip.text)}
              </span>
            </span>
            <button
              onClick={(e) => {
                e.stopPropagation();
                void ttsApi.deleteRecent(clip.id);
              }}
              className="shrink-0 opacity-0 transition-opacity group-hover:opacity-100 hover:text-danger"
              aria-label="Delete"
              title="Delete"
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
        ))}
      </ul>
    </div>
  );
}

function Exports() {
  const s = useTtsState();
  if (!s.player.current) return null;
  return (
    <div className="flex items-center gap-1.5">
      <button
        onClick={() => void ttsApi.downloadWav()}
        className="flex-1 rounded-md border border-border bg-surface px-3 py-1.5 text-xs text-fg hover:border-border-strong"
      >
        {s.strings.downloadWav}
      </button>
      <button
        onClick={() => void ttsApi.downloadMp3()}
        disabled={s.player.isExporting}
        className={clsx(
          "flex-1 rounded-md border border-border bg-surface px-3 py-1.5 text-xs text-fg",
          s.player.isExporting
            ? "cursor-wait text-muted"
            : "hover:border-border-strong",
        )}
      >
        {s.player.isExporting ? s.strings.exporting : s.strings.downloadMp3}
      </button>
    </div>
  );
}

function fmt(sec: number): string {
  const total = Math.max(0, Math.floor(sec));
  const m = Math.floor(total / 60);
  const s = total % 60;
  return `${m}:${s.toString().padStart(2, "0")}`;
}

function flatten(text: string): string {
  return text.replace(/\s+/g, " ").trim().slice(0, 40);
}
