import clsx from "clsx";
import { Card, FormRow, Select } from "./components";
import { ttsApi } from "./ipc";
import { useTtsState } from "./state";

export function AudioEditPanel() {
  const s = useTtsState();
  const e = s.audioEdit;
  const subtasks = s.catalogs.audioEditSubtasksByTask[e.editType] ?? [];
  const isParalinguistic = e.editType === "paralinguistic";
  return (
    <Card title={s.strings.methodStepAudio}>
      <div className="flex flex-wrap gap-1.5">
        <button
          onClick={() => void ttsApi.pickSourceAudio()}
          className="rounded-md border border-border bg-surface px-2.5 py-1 text-xs hover:border-border-strong"
        >
          {s.strings.pickSource}
        </button>
        <button
          onClick={() => void ttsApi.useCurrentAsSource()}
          className="rounded-md border border-border bg-surface px-2.5 py-1 text-xs hover:border-border-strong"
        >
          {s.strings.useCurrent}
        </button>
        <button
          onClick={() => {
            if (s.player.isMicRecording) void ttsApi.stopMicRecording();
            else void ttsApi.startMicRecording();
          }}
          className={clsx(
            "rounded-md border px-2.5 py-1 text-xs",
            s.player.isMicRecording
              ? "border-danger bg-danger/15 text-danger"
              : "border-border bg-surface hover:border-border-strong",
          )}
        >
          {s.player.isMicRecording ? s.strings.stopMic : s.strings.recordMic}
        </button>
      </div>
      <p className="truncate text-[11px] text-muted" title={e.sourcePath}>
        {e.sourcePath || s.strings.noSource}
      </p>
      <FormRow label={s.strings.task}>
        <Select
          value={e.editType}
          options={s.catalogs.audioEditTasks}
          onChange={(editType) => void ttsApi.patchAudioEdit({ editType })}
        />
      </FormRow>
      {subtasks.length > 0 && (
        <FormRow label={s.strings.subtask}>
          <Select
            value={e.editInfo || subtasks[0]?.value || ""}
            options={subtasks}
            onChange={(editInfo) => void ttsApi.patchAudioEdit({ editInfo })}
          />
        </FormRow>
      )}
      {isParalinguistic && (
        <FormRow label={s.strings.inlineSoundTag}>
          <Select
            value=""
            placeholder={s.strings.insertTag + "…"}
            options={s.catalogs.paralinguisticTags.map((t) => ({
              value: t,
              label: t,
            }))}
            onChange={(tag) => {
              if (!tag) return;
              const sep =
                !e.targetText.length || e.targetText.endsWith(" ") ? "" : " ";
              void ttsApi.patchAudioEdit({
                targetText: `${e.targetText}${sep}${tag} `,
              });
            }}
          />
        </FormRow>
      )}
      {isParalinguistic && (
        <div className="flex flex-col gap-1">
          <label className="text-xs text-muted">{s.strings.targetText}</label>
          <textarea
            value={e.targetText}
            onChange={(ev) =>
              void ttsApi.patchAudioEdit({ targetText: ev.target.value })
            }
            className="min-h-[72px] resize-none rounded-md border border-border bg-surface px-2 py-1.5 font-mono text-[12px] outline-none focus:border-accent"
          />
        </div>
      )}
    </Card>
  );
}

export function S2SPanel() {
  const s = useTtsState();
  return (
    <Card title="Gemini S2S">
      <div className="flex flex-wrap gap-1.5">
        <button
          onClick={() => void ttsApi.pickSourceAudio()}
          className="rounded-md border border-border bg-surface px-2.5 py-1 text-xs hover:border-border-strong"
        >
          {s.strings.pickSource}
        </button>
        <button
          onClick={() => {
            if (s.player.isMicRecording) void ttsApi.stopMicRecording();
            else void ttsApi.startMicRecording();
          }}
          className={clsx(
            "rounded-md border px-2.5 py-1 text-xs",
            s.player.isMicRecording
              ? "border-danger bg-danger/15 text-danger"
              : "border-border bg-surface hover:border-border-strong",
          )}
        >
          {s.player.isMicRecording ? s.strings.stopMic : s.strings.recordMic}
        </button>
      </div>
      <p
        className="truncate text-[11px] text-muted"
        title={s.audioEdit.sourcePath}
      >
        {s.audioEdit.sourcePath || s.strings.noSource}
      </p>
      <FormRow label="Target">
        <Select
          value={s.s2sTargetLanguage}
          options={s.catalogs.s2sLanguages}
          onChange={(language) => void ttsApi.setS2sTargetLanguage(language)}
        />
      </FormRow>
    </Card>
  );
}
