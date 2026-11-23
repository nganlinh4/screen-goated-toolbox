import { Button, Card, FormRow, Select } from "./components";
import { ttsApi } from "./ipc";
import { useTtsState } from "./state";

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
        title={s.sourceAudio.sourcePath}
      >
        {s.sourceAudio.sourcePath || s.strings.noSource}
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
