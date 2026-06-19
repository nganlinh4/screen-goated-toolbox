import clsx from "clsx";
import { useTtsState } from "./state";
import { ttsApi } from "./ipc";
import { Button } from "./components";

export function Studio() {
  const s = useTtsState();
  return (
    <div className="tts-studio-panels flex h-full flex-col gap-3">
      {s.mode === "SpeechToSpeech" ? <SpeechToSpeechInput /> : <TextInput />}
      <Player />
      <RecentClips />
      <Exports />
      {s.player.error && (
        <div className="tts-error rounded-md border border-danger/30 bg-danger/10 px-3 py-2 text-xs text-danger">
          {s.player.error}
        </div>
      )}
    </div>
  );
}

function SpeechToSpeechInput() {
  const s = useTtsState();
  const canGenerate = !s.player.isGenerating && Boolean(s.audioEdit.sourcePath);
  return (
    <div className="tts-s2s-input flex flex-col gap-2.5 rounded-lg bg-surface-soft p-3.5 shadow-elevation-2">
      <label className="tts-input-label text-xs font-medium uppercase tracking-wide text-muted">
        Gemini S2S
      </label>
      <p
        className="tts-s2s-source truncate text-xs text-muted"
        title={s.audioEdit.sourcePath}
      >
        {s.audioEdit.sourcePath || s.strings.noSource}
      </p>
      <div className="tts-input-actions flex items-center justify-end gap-2">
        {s.player.isGenerating && (
          <Button variant="ghost" size="sm" className="tts-cancel" onClick={() => void ttsApi.cancelGeneration()}>
            {s.strings.cancel}
          </Button>
        )}
        <Button
          variant="primary"
          size="md"
          className="tts-generate"
          disabled={!canGenerate}
          onClick={() => void ttsApi.generate()}
        >
          {s.player.isGenerating ? s.strings.generating : s.strings.generate}
        </Button>
      </div>
    </div>
  );
}

function TextInput() {
  const s = useTtsState();
  const editMode =
    s.mode === "AudioEdit" && s.audioEdit.editType !== "paralinguistic";
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
    <div className="tts-text-input flex flex-col gap-2.5 rounded-lg bg-surface-soft p-3.5 shadow-elevation-2">
      <label className="tts-input-label text-xs font-medium uppercase tracking-wide text-muted">
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
        className="tts-text-area min-h-[120px] resize-none rounded-md bg-surface px-2.5 py-2 font-mono text-sm leading-relaxed text-fg outline-none transition focus:ring-2 focus:ring-accent/25"
      />
      <div className="tts-text-footer flex items-center justify-between gap-2">
        <span className="tts-char-count text-xs text-muted tabular-nums">
          {(s.strings.charCountTemplate || "{n} chars").replace(
            "{n}",
            String((editMode ? s.audioEdit.sourceText : s.draftText).length),
          )}
        </span>
        <div className="tts-text-actions flex items-center gap-2">
          {s.player.isGenerating && (
            <Button
              variant="ghost"
              size="sm"
              className="tts-cancel"
              onClick={() => void ttsApi.cancelGeneration()}
            >
              {s.strings.cancel}
            </Button>
          )}
          <Button variant="secondary" size="sm" className="tts-clear" onClick={() => void ttsApi.clear()}>
            {s.strings.clear}
          </Button>
          <Button
            variant="primary"
            size="md"
            className="tts-generate"
            disabled={!canGenerate}
            onClick={() => void ttsApi.generate()}
          >
            {s.player.isGenerating ? s.strings.generating : s.strings.generate}
          </Button>
        </div>
      </div>
    </div>
  );
}

function Player() {
  const s = useTtsState();
  const current = s.player.current;
  if (!current) {
    return (
      <div className="tts-player tts-player-empty rounded-lg border border-dashed border-border bg-surface px-3 py-5 text-center text-xs text-muted">
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
    <div className="tts-player flex flex-col gap-2.5 rounded-lg bg-surface-soft p-3.5 shadow-elevation-2">
      <div className="tts-player-meta flex items-baseline justify-between gap-2">
        <span
          className="tts-player-voice truncate text-sm font-semibold text-fg"
          title={current.voiceLabel}
        >
          {current.voiceLabel}
        </span>
        <span className="tts-player-time shrink-0 font-mono text-xs tabular-nums text-muted">
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
        className="tts-player-seek seek-bar"
        style={{
          background: `linear-gradient(to right, rgb(var(--accent)) ${pct}%, rgb(var(--surface-strong)) ${pct}%)`,
        }}
      />
      <div className="tts-player-transport flex items-center gap-2">
        <Button
          variant="primary"
          size="md"
          className="tts-play"
          onClick={() => {
            if (s.player.isPlaying) void ttsApi.pause();
            else void ttsApi.play();
          }}
        >
          {playLabel}
        </Button>
        <Button variant="secondary" size="sm" className="tts-stop" onClick={() => void ttsApi.stop()}>
          {s.strings.stop}
        </Button>
        <Button variant="secondary" size="sm" className="tts-replay" onClick={() => void ttsApi.replay()}>
          {s.strings.replay}
        </Button>
      </div>
    </div>
  );
}

function RecentClips() {
  const s = useTtsState();
  if (s.player.recent.length === 0) return null;
  return (
    <div className="tts-recent flex min-h-0 flex-1 flex-col gap-2 rounded-lg bg-surface-soft p-3.5 shadow-elevation-2">
      <div className="tts-recent-title text-xs font-semibold uppercase tracking-wide text-muted">
        {s.strings.recent}
      </div>
      <ul className="tts-recent-list flex flex-col gap-0.5 overflow-y-auto">
        {s.player.recent.map((clip) => (
          <li
            key={clip.id}
            className="tts-recent-item group flex cursor-pointer items-center justify-between gap-2 rounded-md px-2 py-1.5 transition hover:bg-surface-strong"
            onClick={() => void ttsApi.playRecent(clip.id)}
          >
            <span className="tts-recent-meta flex min-w-0 flex-1 items-center gap-1.5 truncate text-xs">
              <span className="shrink-0 font-mono text-2xs text-muted">
                {clip.createdLabel}
              </span>
              <span className="shrink-0 truncate font-medium text-fg" title={clip.voiceLabel}>
                {clip.voiceLabel}
              </span>
              <span className="shrink-0 font-mono text-2xs text-muted">
                {clip.durationSec.toFixed(1)}s
              </span>
              <span className="truncate text-muted">·</span>
              <span className="truncate text-muted">{flatten(clip.text)}</span>
            </span>
            <button
              onClick={(e) => {
                e.stopPropagation();
                void ttsApi.deleteRecent(clip.id);
              }}
              className="tts-recent-delete shrink-0 rounded p-0.5 text-muted opacity-0 transition-opacity hover:text-danger group-hover:opacity-100"
              aria-label="Delete"
              title="Delete"
            >
              <svg viewBox="0 0 24 24" className="h-3.5 w-3.5" fill="currentColor">
                <path d="M6.4 19L5 17.6l5.6-5.6L5 6.4L6.4 5l5.6 5.6L17.6 5L19 6.4L13.4 12l5.6 5.6l-1.4 1.4l-5.6-5.6z" />
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
    <div className="tts-exports flex items-center gap-2">
      <Button
        variant="secondary"
        size="md"
        className="tts-export-wav flex-1"
        onClick={() => void ttsApi.downloadWav()}
      >
        {s.strings.downloadWav}
      </Button>
      <Button
        variant="secondary"
        size="md"
        className={clsx("tts-export-mp3 flex-1", s.player.isExporting && "cursor-wait")}
        disabled={s.player.isExporting}
        onClick={() => void ttsApi.downloadMp3()}
      >
        {s.player.isExporting ? s.strings.exporting : s.strings.downloadMp3}
      </Button>
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
