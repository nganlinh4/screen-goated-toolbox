import { useEffect, useState } from "react";
import { invoke } from "@/lib/ipc";
import type {
  BackgroundConfig,
  ProjectComposition,
  ProjectCompositionClip,
  VideoSegment,
} from "@/types/video";
import type { NativeVideoMetadataProbe } from "@/hooks/exportHookUtils";

export interface ExportCompositionDialogState {
  segment: VideoSegment | null;
  backgroundConfig: BackgroundConfig | null;
  trimmedDurationSec: number;
  clipCount: number;
  hasAudio: boolean;
}

interface ExportSourceMetadataOptions {
  showExportDialog: boolean;
  isCompositionExport: boolean;
  composition: ProjectComposition | null;
  lastCaptureFps: number | null;
  resolveClipExportSourcePath: (
    clip: ProjectCompositionClip,
  ) => Promise<string>;
  resolveSourceVideoPath: () => string;
}

export function useExportSourceMetadata({
  showExportDialog,
  isCompositionExport,
  composition,
  lastCaptureFps,
  resolveClipExportSourcePath,
  resolveSourceVideoPath,
}: ExportSourceMetadataOptions) {
  const [sourceVideoFps, setSourceVideoFps] = useState<number | null>(null);
  const [compositionDialogState, setCompositionDialogState] =
    useState<ExportCompositionDialogState | null>(null);

  useEffect(() => {
    if (!showExportDialog) return;
    let cancelled = false;

    if (isCompositionExport && composition) {
      void import("@/lib/compositionExport")
        .then(({ buildCompositionExportDialogState }) =>
          buildCompositionExportDialogState(
            composition,
            resolveClipExportSourcePath,
          ),
        )
        .then((state) => {
          if (cancelled) return;
          setCompositionDialogState({
            segment: state.segment,
            backgroundConfig: state.backgroundConfig,
            trimmedDurationSec: state.trimmedDurationSec,
            clipCount: state.clipCount,
            hasAudio: state.hasAudio,
          });
          setSourceVideoFps(lastCaptureFps ?? state.sourceFps);
        })
        .catch((error) => {
          if (cancelled) return;
          console.warn("[Export] Composition export summary failed:", error);
          setCompositionDialogState(null);
          setSourceVideoFps(lastCaptureFps ?? null);
        });
      return () => { cancelled = true; };
    }

    const sourceVideoPath = resolveSourceVideoPath();
    setCompositionDialogState(null);
    if (!sourceVideoPath) {
      setSourceVideoFps(null);
      return;
    }
    void invoke<Partial<NativeVideoMetadataProbe>>("probe_video_metadata", {
      path: sourceVideoPath,
    })
      .then((metadata) => {
        if (cancelled) return;
        const probedFps =
          typeof metadata?.fps === "number" &&
          Number.isFinite(metadata.fps) &&
          metadata.fps > 0
            ? metadata.fps
            : null;
        setSourceVideoFps(lastCaptureFps ?? probedFps);
      })
      .catch((error) => {
        if (cancelled) return;
        console.warn("[Export] Source video metadata probe failed:", error);
        setSourceVideoFps(lastCaptureFps ?? null);
      });

    return () => { cancelled = true; };
  }, [
    composition,
    isCompositionExport,
    lastCaptureFps,
    resolveClipExportSourcePath,
    resolveSourceVideoPath,
    showExportDialog,
  ]);

  return { compositionDialogState, sourceVideoFps };
}
