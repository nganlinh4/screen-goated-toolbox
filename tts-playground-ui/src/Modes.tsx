import { Button, Card, FormRow, Select } from "./components";
import { ttsApi } from "./ipc";
import { useTtsState } from "./state";

export function AudioEditPanel() {
  const s = useTtsState();
  const e = s.audioEdit;
  const subtasks = s.catalogs.audioEditSubtasksByTask[e.editType] ?? [];
  const isParalinguistic = e.editType === "paralinguistic";
  return (
    <Card title={s.strings.methodStepAudio} className="tts-panel-audio-edit">
      <div className="tts-source-actions flex flex-wrap gap-1.5">
        <Button variant="secondary" size="sm" className="tts-pick-source" onClick={() => void ttsApi.pickSourceAudio()}>
          {s.strings.pickSource}
        </Button>
        <Button variant="secondary" size="sm" className="tts-use-current" onClick={() => void ttsApi.useCurrentAsSource()}>
          {s.strings.useCurrent}
        </Button>
        <Button
          variant={s.player.isMicRecording ? "danger" : "secondary"}
          size="sm"
          className="tts-record-mic"
          onClick={() => {
            if (s.player.isMicRecording) void ttsApi.stopMicRecording();
            else void ttsApi.startMicRecording();
          }}
        >
          {s.player.isMicRecording ? s.strings.stopMic : s.strings.recordMic}
        </Button>
      </div>
      <p className="tts-source-path truncate text-xs text-muted" title={e.sourcePath}>
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
        <div className="tts-target-text flex flex-col gap-1.5">
          <label className="tts-target-label text-xs font-medium text-muted">
            {s.strings.targetText}
          </label>
          <textarea
            value={e.targetText}
            onChange={(ev) =>
              void ttsApi.patchAudioEdit({ targetText: ev.target.value })
            }
            className="tts-target-area min-h-[72px] resize-none rounded-md bg-surface px-2.5 py-2 font-mono text-sm leading-relaxed text-fg outline-none transition focus:ring-2 focus:ring-accent/25"
          />
        </div>
      )}
    </Card>
  );
}

export function S2SPanel() {
  const s = useTtsState();
  return (
    <Card title="Gemini S2S" className="tts-panel-s2s">
      <div className="tts-source-actions flex flex-wrap gap-1.5">
        <Button variant="secondary" size="sm" className="tts-pick-source" onClick={() => void ttsApi.pickSourceAudio()}>
          {s.strings.pickSource}
        </Button>
        <Button
          variant={s.player.isMicRecording ? "danger" : "secondary"}
          size="sm"
          className="tts-record-mic"
          onClick={() => {
            if (s.player.isMicRecording) void ttsApi.stopMicRecording();
            else void ttsApi.startMicRecording();
          }}
        >
          {s.player.isMicRecording ? s.strings.stopMic : s.strings.recordMic}
        </Button>
      </div>
      <p
        className="tts-source-path truncate text-xs text-muted"
        title={s.audioEdit.sourcePath}
      >
        {s.audioEdit.sourcePath || s.strings.noSource}
      </p>
      <FormRow label={s.strings.s2sTarget}>
        <Select
          value={s.s2sTargetLanguage}
          options={s.catalogs.s2sLanguages}
          onChange={(language) => void ttsApi.setS2sTargetLanguage(language)}
        />
      </FormRow>
    </Card>
  );
}
