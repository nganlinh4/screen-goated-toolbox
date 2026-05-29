import { Card, SmallButton } from "./components";
import { ttsApi } from "./ipc";
import { useTtsState } from "./state";
import type { ReferenceVoice } from "./types";

export function ReferenceLibraryPanel() {
  const s = useTtsState();
  const stepActive = s.stepAudio.reference;
  return (
    <div className="tts-reference-library flex flex-col gap-3">
      <Card
        title={s.strings.modeReferenceLibrary}
        description={s.strings.referenceLibraryDesc}
        className="tts-panel-references-step"
        action={
          <SmallButton onClick={() => void ttsApi.addReference()}>
            {s.strings.referenceAdd}
          </SmallButton>
        }
      >
        <ReferenceList
          items={s.catalogs.stepAudioReferences}
          activeId={stepActive}
          emptyLabel={s.strings.referenceEmpty}
          target="stepAudio"
        />
      </Card>
      <Card title={s.strings.methodVieneu} className="tts-panel-references-vieneu">
        <ReferenceList
          items={s.catalogs.vieneuReferences}
          activeId={s.vieneu.reference}
          emptyLabel={s.strings.referenceEmpty}
          target="vieneu"
        />
      </Card>
    </div>
  );
}

function ReferenceList({
  items,
  activeId,
  emptyLabel,
  target,
}: {
  items: ReferenceVoice[];
  activeId: string;
  emptyLabel: string;
  target: "stepAudio" | "vieneu";
}) {
  const s = useTtsState();
  if (items.length === 0) {
    return <p className="tts-reference-empty text-xs text-muted">{emptyLabel}</p>;
  }
  return (
    <ul className="tts-reference-list flex flex-col gap-2">
      {items.map((r) => {
        const active = r.id === activeId;
        return (
          <li
            key={r.id}
            className={
              "tts-reference-item flex flex-col gap-2 rounded-md p-2.5 text-xs shadow-elevation-1 " +
              (active
                ? "tts-reference-item--active bg-accent-soft/60 ring-1 ring-accent"
                : "bg-surface-soft")
            }
          >
            <div className="tts-reference-head grid grid-cols-[1fr,auto] gap-2">
              <input
                value={r.name}
                aria-label={s.strings.referenceLabel}
                onChange={(ev) =>
                  void ttsApi.updateReference(r.id, { label: ev.target.value })
                }
                className="tts-reference-name rounded-md bg-surface px-2.5 py-1.5 font-medium text-fg outline-none transition focus:ring-2 focus:ring-accent/25"
              />
              <SmallButton
                onClick={() => {
                  if (target === "vieneu") {
                    void ttsApi.patchVieneu({ reference: r.id });
                  } else {
                    void ttsApi.patchStepAudio({ reference: r.id });
                  }
                }}
              >
                {r.name}
              </SmallButton>
            </div>
            <div className="tts-reference-actions flex flex-wrap gap-1.5">
              <SmallButton onClick={() => void ttsApi.pickReferenceAudio(r.id)}>
                {s.strings.referencePickAudio}
              </SmallButton>
              <SmallButton onClick={() => void ttsApi.playReference(r.id)}>
                {s.strings.play}
              </SmallButton>
              <SmallButton
                onClick={() => {
                  if (s.player.isMicRecording) void ttsApi.stopReferenceMic();
                  else void ttsApi.startReferenceMic(r.id);
                }}
              >
                {s.player.isMicRecording ? s.strings.stopMic : s.strings.recordMic}
              </SmallButton>
              <SmallButton onClick={() => void ttsApi.recognizeReference(r.id)}>
                {s.strings.referenceAutoRecognize}
              </SmallButton>
              <SmallButton onClick={() => void ttsApi.useReference(r.id, "playground")}>
                {s.strings.referenceUsePlayground}
              </SmallButton>
              <SmallButton onClick={() => void ttsApi.useReference(r.id, "global")}>
                {s.strings.referenceUseGlobal}
              </SmallButton>
              <SmallButton onClick={() => void ttsApi.deleteReference(r.id)}>
                {s.strings.delete}
              </SmallButton>
            </div>
            <p className="tts-reference-audio truncate text-xs text-muted" title={r.audioPath}>
              {r.audioPath || s.strings.referenceNoAudio}
            </p>
            <textarea
              value={r.transcript || ""}
              onChange={(ev) =>
                void ttsApi.updateReference(r.id, {
                  transcript: ev.target.value,
                })
              }
              placeholder={s.strings.referenceExactTranscript}
              className="tts-reference-transcript min-h-[52px] resize-none rounded-md bg-surface px-2.5 py-1.5 text-xs text-fg outline-none transition focus:ring-2 focus:ring-accent/25"
            />
            <span
              className="tts-reference-id truncate font-mono text-2xs text-muted"
              title={r.id}
            >
              {r.id}
            </span>
          </li>
        );
      })}
    </ul>
  );
}
