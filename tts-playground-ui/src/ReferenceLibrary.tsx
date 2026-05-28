import { Card, SmallButton } from "./components";
import { ttsApi } from "./ipc";
import { useTtsState } from "./state";
import type { ReferenceVoice } from "./types";

export function ReferenceLibraryPanel() {
  const s = useTtsState();
  const stepActive = s.stepAudio.reference;
  return (
    <div className="flex flex-col gap-3">
      <Card
        title={s.strings.modeReferenceLibrary}
        description={s.strings.referenceLibraryDesc}
        action={
          <SmallButton onClick={() => void ttsApi.addReference()}>
            {s.strings.referenceAdd}
          </SmallButton>
        }
      >
        <ReferenceList
          items={s.catalogs.stepAudioReferences}
          activeId={stepActive}
          emptyLabel="No VieNeu references saved yet."
          target="stepAudio"
        />
      </Card>
      <Card title={s.strings.methodVieneu}>
        <ReferenceList
          items={s.catalogs.vieneuReferences}
          activeId={s.vieneu.reference}
          emptyLabel="No VieNeu references saved yet."
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
    return <p className="text-xs text-muted">{emptyLabel}</p>;
  }
  return (
    <ul className="flex flex-col gap-2">
      {items.map((r) => {
        const active = r.id === activeId;
        return (
          <li
            key={r.id}
            className={
              "flex flex-col gap-2 rounded border p-2 text-xs " +
              (active ? "border-accent/70 bg-accent/10" : "border-border/60 bg-surface")
            }
          >
            <div className="grid grid-cols-[1fr,auto] gap-2">
              <input
                value={r.name}
                aria-label={s.strings.referenceLabel}
                onChange={(ev) =>
                  void ttsApi.updateReference(r.id, { label: ev.target.value })
                }
                className="rounded border border-border bg-surface-soft px-2 py-1 font-medium outline-none focus:border-accent"
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
            <div className="flex flex-wrap gap-1.5">
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
                Delete
              </SmallButton>
            </div>
            <p className="truncate text-[11px] text-muted" title={r.audioPath}>
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
              className="min-h-[52px] resize-none rounded border border-border bg-surface-soft px-2 py-1 text-[11px] outline-none focus:border-accent"
            />
            <span
              className="truncate font-mono text-[10px] text-muted"
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
