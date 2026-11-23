import { useEffect, useRef, useState } from "react";
import { useSettings } from "@/hooks/useSettings";
import { invoke } from "@/lib/ipc";
import { recordingHandoff, waitForRecordingAck } from "@/lib/recordingHandoff";

interface FailedRecording {
  id: number;
  error: string;
  directory: string;
  paths: string[];
  canRelease: boolean;
}

export function RecordingRecoveryNotice() {
  const { t } = useSettings();
  const [failure, setFailure] = useState<FailedRecording | null>(null);
  const [busy, setBusy] = useState(false);
  const [retryRequired, setRetryRequired] = useState(false);
  const preserveRetry = useRef(false);

  useEffect(() => {
    let mounted = true;
    const refresh = () => {
      if (preserveRetry.current) return;
      void invoke<FailedRecording | null>("get_failed_recording").then((value) => {
        if (mounted && !preserveRetry.current) setFailure(value);
      }).catch(() => {});
    };
    refresh();
    const timer = window.setInterval(refresh, 2000);
    const requireRetry = () => setRetryRequired(true);
    const clearRetry = () => setRetryRequired(false);
    window.addEventListener("sr-shutdown-failed", refresh);
    window.addEventListener("sr-recording-retry-required", requireRetry);
    window.addEventListener("sr-recording-retry-cleared", clearRetry);
    window.addEventListener("sr-abandon-recording", clearRetry);
    return () => {
      mounted = false;
      window.clearInterval(timer);
      window.removeEventListener("sr-shutdown-failed", refresh);
      window.removeEventListener("sr-recording-retry-required", requireRetry);
      window.removeEventListener("sr-recording-retry-cleared", clearRetry);
      window.removeEventListener("sr-abandon-recording", clearRetry);
    };
  }, []);

  const keepFiles = async () => {
    if (!failure || !recordingHandoff.beginRelease()) return;
    preserveRetry.current = true;
    setBusy(true);
    let released = false;
    try {
      await waitForRecordingAck(invoke("release_failed_recording", { id: failure.id }));
      released = true;
      window.dispatchEvent(new Event("sr-abandon-recording"));
      setFailure(null);
      preserveRetry.current = false;
    } catch (error) {
      console.error("Failed to acknowledge retained recording files", error);
    } finally {
      recordingHandoff.finishRelease(released);
      setBusy(false);
    }
  };

  if (!failure && !retryRequired) return null;
  return (
    <section className="recording-recovery-notice mb-2 shrink-0 rounded-lg border border-[var(--tertiary-color)] bg-[var(--ui-surface-3)] p-3 text-sm text-[var(--on-surface)]" role="alert">
      {retryRequired && (
        <div className="recording-retry-controls">
          <p className="recording-retry-description">{t.recordingRetryDescription}</p>
          <button className="recording-retry-button mt-2 rounded-md border border-[var(--ui-border)] px-3 py-1.5" onClick={() => window.dispatchEvent(new Event("sr-recover-recording"))}>
            {t.recordingRetryButton}
          </button>
        </div>
      )}
      {failure && <>
      <p className="recording-recovery-description">{t.failedRecordingFiles}</p>
      <p className="recording-recovery-directory">{failure.directory}</p>
      {failure.paths.map((path) => <p className="recording-recovery-path" key={path}>{path}</p>)}
      <p className="recording-recovery-error">{failure.error}</p>
      {failure.canRelease ? (
        <button className="recording-recovery-keep mt-2 rounded-md border border-[var(--ui-border)] px-3 py-1.5" disabled={busy} onClick={() => void keepFiles()}>
          {t.failedRecordingKeep}
        </button>
      ) : <p className="recording-recovery-wait">{t.failedRecordingWait}</p>}
      </>}
    </section>
  );
}
