import { useEffect, type MutableRefObject } from "react";
import { invoke } from "@/lib/ipc";
import { recordingHandoff } from "@/lib/recordingHandoff";
import { PREVIEW_MEDIA_MOUNTED, type PreviewMediaElements } from "@/lib/previewMediaLifecycle";
import { sanitizeRecordingAudioSelection, type RecordingAudioSelection } from "@/types/recordingAudio";
import type { RecordingMode } from "@/types/video";
import type { NativeRecordingResult, UseRecordingProps } from "./useRecording";

interface ActiveRecordingRecovery { recordingActive: true; recordingMode: RecordingMode }

export function usePendingRecordingRecovery(
  props: Pick<UseRecordingProps, "videoControllerRef" | "videoRef" | "canvasRef">,
  active: MutableRefObject<boolean>,
  mode: MutableRefObject<RecordingMode>,
  pending: MutableRefObject<NativeRecordingResult | null>,
  audio: MutableRefObject<RecordingAudioSelection>,
): void {
  const { videoControllerRef, videoRef, canvasRef } = props;
  useEffect(() => {
    let cancelled = false;
    let needsRecovery = false;
    const recover = () => {
      if (cancelled || !needsRecovery || !videoControllerRef.current) return;
      needsRecovery = false;
      window.dispatchEvent(new Event("sr-recover-recording"));
    };
    const mounted = (event: Event) => {
      const detail = (event as CustomEvent<PreviewMediaElements>).detail;
      if (detail?.video === videoRef.current && detail?.canvas === canvasRef.current) recover();
    };
    window.addEventListener(PREVIEW_MEDIA_MOUNTED, mounted);
    void invoke<NativeRecordingResult | ActiveRecordingRecovery | null>("get_pending_recording").then((result) => {
      if (cancelled || !result || active.current) return;
      if ("recordingActive" in result) {
        mode.current = result.recordingMode;
        active.current = true;
        recordingHandoff.restorePending();
        window.dispatchEvent(new Event("sr-recording-retry-required"));
        return;
      }
      if (!/^[0-9a-f]{32}$/.test(result.recordingId) || !result.videoFilePath || !result.videoUrl) return;
      pending.current = result;
      mode.current = result.recordingMode;
      active.current = true;
      audio.current = sanitizeRecordingAudioSelection({
        deviceEnabled: true, micEnabled: Boolean(result.micAudioPath || result.micAudioUrl),
        deviceMode: "all", selectedDeviceApp: null,
      });
      recordingHandoff.restorePending();
      needsRecovery = true;
      recover();
    }).catch((error) => console.error("Failed to recover recording", error));
    return () => { cancelled = true; window.removeEventListener(PREVIEW_MEDIA_MOUNTED, mounted); };
  }, [videoControllerRef, videoRef, canvasRef, active, mode, pending, audio]);
}
