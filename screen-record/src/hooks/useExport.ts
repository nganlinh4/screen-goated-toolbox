import { useState, useEffect, useCallback } from "react";
import { invoke } from "@/lib/ipc";
import { cloneBackgroundConfig } from "@/lib/backgroundConfig";
import {
  buildCompositionExportDialogState,
  exportCompositionAndDownload,
} from "@/lib/compositionExport";
import { videoExporter } from "@/lib/videoExporter";
import {
  BackgroundConfig,
  ExportArtifact,
  VideoSegment,
  MousePosition,
  ExportOptions,
  ProjectComposition,
  ProjectCompositionClip,
  WebcamConfig,
} from "@/types/video";
import { getTotalTrimDuration } from "@/lib/trimSegments";
import { cloneWebcamConfig } from "@/lib/webcam";
import { getSavedExportFpsPref } from "./videoStatePreferences";

// ============================================================================
// useExport
// ============================================================================
interface UseExportProps {
  videoRef: React.RefObject<HTMLVideoElement | null>;
  webcamVideoRef: React.RefObject<HTMLVideoElement | null>;
  canvasRef: React.RefObject<HTMLCanvasElement | null>;
  tempCanvasRef: React.RefObject<HTMLCanvasElement>;
  audioRef: React.RefObject<HTMLAudioElement | null>;
  micAudioRef: React.RefObject<HTMLAudioElement | null>;
  isRecording: boolean;
  isBatchEditing: boolean;
  segment: VideoSegment | null;
  backgroundConfig: BackgroundConfig;
  webcamConfig: WebcamConfig;
  mousePositions: MousePosition[];
  audioFilePath: string;
  micAudioFilePath: string;
  webcamVideoFilePath: string;
  videoFilePath: string;
  videoFilePathOwnerUrl: string;
  rawVideoPath: string;
  savedRawVideoPath: string;
  currentVideo: string | null;
  /** Actual FPS the most-recent recording was encoded at (from backend). Overrides probe. */
  lastCaptureFps: number | null;
  composition: ProjectComposition | null;
  currentProjectId: string | null;
  resolveClipExportSourcePath: (
    clip: ProjectCompositionClip,
  ) => Promise<string>;
  resolveClipExportMicAudioPath: (
    clip: ProjectCompositionClip,
  ) => Promise<string>;
  resolveClipExportWebcamPath: (
    clip: ProjectCompositionClip,
  ) => Promise<string>;
}

interface NativeVideoMetadataProbe {
  width: number;
  height: number;
  fps: number;
  fpsNum: number;
  fpsDen: number;
}

export function useExport(props: UseExportProps) {
  const [isProcessing, setIsProcessing] = useState(false);
  const [exportProgress, setExportProgress] = useState(0);
  const [showExportDialog, setShowExportDialog] = useState(false);
  const [showExportSuccessDialog, setShowExportSuccessDialog] = useState(false);
  const [lastExportedPath, setLastExportedPath] = useState("");
  const [lastExportArtifacts, setLastExportArtifacts] = useState<
    ExportArtifact[]
  >([]);
  const [sourceVideoFps, setSourceVideoFps] = useState<number | null>(null);
  const [compositionDialogState, setCompositionDialogState] = useState<{
    segment: VideoSegment | null;
    backgroundConfig: BackgroundConfig | null;
    trimmedDurationSec: number;
    clipCount: number;
    hasAudio: boolean;
  } | null>(null);
  const [exportAutoCopyEnabled, setExportAutoCopyEnabled] = useState(() => {
    try {
      return localStorage.getItem("screen-record-export-auto-copy-v1") === "1";
    } catch {
      return false;
    }
  });
  const [exportOptions, setExportOptions] = useState<ExportOptions>(() => {
    return {
      width: 0,
      height: 0,
      // Preserve user-selected source-matched FPS (for example 50fps recordings),
      // instead of restricting to a fixed preset list.
      fps: getSavedExportFpsPref(),
      targetVideoBitrateKbps: 0,
      speed: 1,
      exportProfile: "turbo_nv",
      preferNvTurbo: true,
      qualityGatePercent: 3,
      turboCodec: "hevc",
      preRenderPolicy: "aggressive",
      outputDir: "",
      format: "mp4",
    };
  });
  const [hasCheckedExportCapabilities, setHasCheckedExportCapabilities] =
    useState(false);

  useEffect(() => {
    try {
      localStorage.setItem(
        "screen-record-export-auto-copy-v1",
        exportAutoCopyEnabled ? "1" : "0",
      );
    } catch {}
  }, [exportAutoCopyEnabled]);

  const handleExport = useCallback(() => setShowExportDialog(true), []);
  const isCompositionExport = (props.composition?.clips.length ?? 0) > 1;

  const resolveSourceVideoPath = useCallback((): string => {
    const directRecordingPath =
      props.currentVideo === props.videoFilePathOwnerUrl
        ? props.videoFilePath
        : "";
    return (
      directRecordingPath ||
      props.rawVideoPath ||
      props.savedRawVideoPath ||
      ""
    ).trim();
  }, [
    props.currentVideo,
    props.videoFilePathOwnerUrl,
    props.videoFilePath,
    props.rawVideoPath,
    props.savedRawVideoPath,
  ]);

  useEffect(() => {
    if (!showExportDialog) return;

    if (isCompositionExport && props.composition) {
      let cancelled = false;
      void buildCompositionExportDialogState(
        props.composition,
        props.resolveClipExportSourcePath,
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
          setSourceVideoFps(props.lastCaptureFps ?? state.sourceFps);
        })
        .catch((error) => {
          if (cancelled) return;
          console.warn("[Export] Composition export summary failed:", error);
          setCompositionDialogState(null);
          setSourceVideoFps(props.lastCaptureFps ?? null);
        });
      return () => {
        cancelled = true;
      };
    }

    const sourceVideoPath = resolveSourceVideoPath();
    setCompositionDialogState(null);
    if (!sourceVideoPath) {
      setSourceVideoFps(null);
      return;
    }

    let cancelled = false;
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
        // Prefer the authoritative capture FPS from the backend over the container
        // metadata probe — WinRT encoder may write 60fps headers even for 100fps captures.
        setSourceVideoFps(props.lastCaptureFps ?? probedFps);
      })
      .catch((error) => {
        if (cancelled) return;
        console.warn("[Export] Source video metadata probe failed:", error);
        setSourceVideoFps(props.lastCaptureFps ?? null);
      });

    return () => {
      cancelled = true;
    };
  }, [
    isCompositionExport,
    props.composition,
    props.lastCaptureFps,
    props.resolveClipExportSourcePath,
    resolveSourceVideoPath,
    showExportDialog,
  ]);

  useEffect(() => {
    let cancelled = false;
    void videoExporter
      .getExportCapabilities()
      .then((caps) => {
        if (cancelled) return;
        setExportOptions((prev) => {
          if (!caps.nvencAvailable) {
            if (prev.exportProfile === "turbo_nv" || prev.preferNvTurbo) {
              return {
                ...prev,
                exportProfile: "max_speed",
                preferNvTurbo: false,
                turboCodec: "h264",
              };
            }
            return prev;
          }
          if (caps.nvencAvailable && prev.exportProfile !== "turbo_nv") {
            return {
              ...prev,
              exportProfile: "turbo_nv",
              preferNvTurbo: true,
              turboCodec: caps.hevcNvencAvailable ? "hevc" : "h264",
            };
          }
          if (!caps.hevcNvencAvailable && prev.turboCodec === "hevc") {
            return {
              ...prev,
              turboCodec: "h264",
            };
          }
          return prev;
        });
        setHasCheckedExportCapabilities(true);
      })
      .catch((error) => {
        if (cancelled) return;
        console.warn(
          "[Export] capability probe failed, using safe defaults:",
          error,
        );
        setExportOptions((prev) => {
          if (prev.exportProfile !== "turbo_nv") return prev;
          return {
            ...prev,
            exportProfile: "max_speed",
            preferNvTurbo: false,
            turboCodec: "h264",
          };
        });
        setHasCheckedExportCapabilities(true);
      });

    return () => {
      cancelled = true;
    };
  }, []);

  // Shared builder for the prime-preparation args (used by both idle + dialog priming effects).
  const buildPrimeArgs = useCallback(
    (videoEl: HTMLVideoElement, canvasEl: HTMLCanvasElement, segment: VideoSegment, sourceVideoPath: string) => ({
      width: exportOptions.width,
      height: exportOptions.height,
      fps: exportOptions.fps,
      targetVideoBitrateKbps: exportOptions.targetVideoBitrateKbps,
      speed: exportOptions.speed,
      exportProfile: exportOptions.exportProfile || "turbo_nv",
      preferNvTurbo: exportOptions.preferNvTurbo ?? true,
      qualityGatePercent: exportOptions.qualityGatePercent ?? 3,
      turboCodec: exportOptions.turboCodec || "hevc",
      preRenderPolicy: exportOptions.preRenderPolicy || "idle_only",
      outputDir: exportOptions.outputDir || "",
      video: videoEl,
      canvas: canvasEl,
      tempCanvas: props.tempCanvasRef.current!,
      segment,
      backgroundConfig: cloneBackgroundConfig(props.backgroundConfig),
      webcamConfig: cloneWebcamConfig(props.webcamConfig),
      mousePositions: props.mousePositions,
      audio: props.audioRef.current || undefined,
      micAudio: props.micAudioRef.current || undefined,
      webcamVideo: props.webcamVideoRef.current || undefined,
      audioFilePath: props.audioFilePath || sourceVideoPath,
      micAudioFilePath: props.micAudioFilePath || "",
      webcamVideoFilePath: props.webcamVideoFilePath || "",
      videoFilePath: sourceVideoPath,
    }),
    [
      exportOptions.width,
      exportOptions.height,
      exportOptions.fps,
      exportOptions.targetVideoBitrateKbps,
      exportOptions.speed,
      exportOptions.exportProfile,
      exportOptions.preferNvTurbo,
      exportOptions.qualityGatePercent,
      exportOptions.turboCodec,
      exportOptions.preRenderPolicy,
      exportOptions.outputDir,
      props.tempCanvasRef,
      props.backgroundConfig,
      props.webcamConfig,
      props.mousePositions,
      props.audioRef,
      props.audioFilePath,
      props.micAudioRef,
      props.micAudioFilePath,
      props.webcamVideoRef,
      props.webcamVideoFilePath,
    ],
  );

  useEffect(() => {
    if (
      props.isRecording ||
      props.isBatchEditing ||
      isProcessing ||
      showExportDialog ||
      isCompositionExport ||
      !hasCheckedExportCapabilities
    )
      return;
    const videoEl = props.videoRef.current;
    const canvasEl = props.canvasRef.current;
    const segment = props.segment;
    if (!props.currentVideo || !segment || !videoEl || !canvasEl) return;

    const sourceVideoPath = resolveSourceVideoPath();
    let cancelled = false;
    const runPrime = () => {
      if (cancelled) return;
      void videoExporter
        .primeExportPreparation(buildPrimeArgs(videoEl, canvasEl, segment, sourceVideoPath))
        .catch(() => {
          // keep background prewarm silent
        });
    };

    const preRenderPolicy = exportOptions.preRenderPolicy || "idle_only";
    if (preRenderPolicy === "off") {
      return () => {
        cancelled = true;
      };
    }

    let idleId = 0;
    const idleApi = window as Window & {
      requestIdleCallback?: (
        cb: () => void,
        options?: { timeout: number },
      ) => number;
      cancelIdleCallback?: (id: number) => void;
    };
    if (preRenderPolicy === "aggressive") {
      idleId = window.setTimeout(runPrime, 80);
    } else if (typeof idleApi.requestIdleCallback === "function") {
      idleId = idleApi.requestIdleCallback(runPrime, { timeout: 1500 });
    } else {
      idleId = window.setTimeout(runPrime, 700);
    }

    return () => {
      cancelled = true;
      if (typeof idleApi.cancelIdleCallback === "function") {
        idleApi.cancelIdleCallback(idleId);
      } else {
        window.clearTimeout(idleId);
      }
    };
  }, [
    props.isRecording,
    props.isBatchEditing,
    isProcessing,
    showExportDialog,
    isCompositionExport,
    hasCheckedExportCapabilities,
    props.currentVideo,
    props.segment,
    props.videoRef,
    props.canvasRef,
    buildPrimeArgs,
    resolveSourceVideoPath,
  ]);

  useEffect(() => {
    if (
      props.isRecording ||
      props.isBatchEditing ||
      isProcessing ||
      !showExportDialog ||
      isCompositionExport ||
      !hasCheckedExportCapabilities
    )
      return;
    const preRenderPolicy = exportOptions.preRenderPolicy || "idle_only";
    if (preRenderPolicy === "off") return;
    const videoEl = props.videoRef.current;
    const canvasEl = props.canvasRef.current;
    const segment = props.segment;
    if (!props.currentVideo || !segment || !videoEl || !canvasEl) return;

    const sourceVideoPath = resolveSourceVideoPath();
    const primeDelayMs = preRenderPolicy === "aggressive" ? 32 : 220;
    const timer = window.setTimeout(() => {
      void videoExporter
        .primeExportPreparation(buildPrimeArgs(videoEl, canvasEl, segment, sourceVideoPath))
        .catch((error) => {
          console.error("[ExportPrep] Warm preparation failed:", error);
        });
    }, primeDelayMs);

    return () => {
      window.clearTimeout(timer);
    };
  }, [
    props.isRecording,
    props.isBatchEditing,
    isProcessing,
    showExportDialog,
    isCompositionExport,
    hasCheckedExportCapabilities,
    props.currentVideo,
    props.segment,
    props.videoRef,
    props.canvasRef,
    buildPrimeArgs,
    resolveSourceVideoPath,
  ]);

  const resolveExportArtifacts = useCallback(
    (
      result:
        | {
            status?: string;
            path?: string;
            artifacts?: ExportArtifact[];
          }
        | undefined,
    ): ExportArtifact[] => {
      if (Array.isArray(result?.artifacts) && result.artifacts.length > 0) {
        return result.artifacts;
      }
      if (typeof result?.path === "string" && result.path) {
        return [
          {
            format: result.path.toLowerCase().endsWith(".gif") ? "gif" : "mp4",
            path: result.path,
            primary: true,
          },
        ];
      }
      return [];
    },
    [],
  );

  const startExport = useCallback(async () => {
    const useBatchExport =
      !!props.composition &&
      (isCompositionExport || (exportOptions.format || "mp4") === "both");
    if (
      !useBatchExport &&
      (!props.currentVideo ||
        !props.segment ||
        !props.videoRef.current ||
        !props.canvasRef.current)
    )
      return;
    const sourceVideoPath = resolveSourceVideoPath();

    try {
      setShowExportDialog(false);
      setIsProcessing(true);
      setLastExportArtifacts([]);
      await new Promise<void>((resolve) =>
        requestAnimationFrame(() => resolve()),
      );

      const res = useBatchExport && props.composition
        ? await exportCompositionAndDownload({
            composition: props.composition,
            exportOptions,
            resolveClipSourcePath: props.resolveClipExportSourcePath,
            resolveClipMicAudioPath: props.resolveClipExportMicAudioPath,
            resolveClipWebcamPath: props.resolveClipExportWebcamPath,
          })
        : await videoExporter.exportAndDownload({
            ...buildPrimeArgs(props.videoRef.current!, props.canvasRef.current!, props.segment!, sourceVideoPath),
            format: exportOptions.format || "mp4",
            onProgress: setExportProgress,
          });
      const artifacts = resolveExportArtifacts(res);
      const primaryArtifact =
        artifacts.find((artifact) => artifact.primary) ?? artifacts[0];
      if (
        res?.status === "success" &&
        primaryArtifact?.path
      ) {
        setLastExportArtifacts(artifacts);
        setLastExportedPath(primaryArtifact.path);
        setShowExportSuccessDialog(true);
        if (exportAutoCopyEnabled) {
          invoke("copy_video_file_to_clipboard", {
            filePath: primaryArtifact.path,
          }).catch(console.error);
        }
      }
    } catch (error) {
      console.error("[Export] Error:", error);
    } finally {
      setIsProcessing(false);
      setExportProgress(0);
    }
  }, [
    exportAutoCopyEnabled,
    exportOptions,
    isCompositionExport,
    props,
    resolveExportArtifacts,
    resolveSourceVideoPath,
  ]);

  const cancelExport = useCallback(() => {
    videoExporter.cancel();
    setIsProcessing(false);
    setExportProgress(0);
  }, []);

  const dialogSegment =
    isCompositionExport && compositionDialogState
      ? compositionDialogState.segment
      : props.segment;
  const dialogBackgroundConfig =
    isCompositionExport && compositionDialogState?.backgroundConfig
      ? compositionDialogState.backgroundConfig
      : props.backgroundConfig;
  const dialogTrimmedDurationSec =
    isCompositionExport && compositionDialogState
      ? compositionDialogState.trimmedDurationSec
      : props.segment
        ? getTotalTrimDuration(
            props.segment,
            props.videoRef.current?.duration || props.segment.trimEnd,
          )
        : 0;
  const dialogClipCount =
    isCompositionExport && compositionDialogState
      ? compositionDialogState.clipCount
      : 1;
  const hasAudio =
    isCompositionExport && compositionDialogState
      ? compositionDialogState.hasAudio
      : Boolean(resolveSourceVideoPath());

  return {
    isProcessing,
    exportProgress,
    showExportDialog,
    setShowExportDialog,
    exportOptions,
    setExportOptions,
    handleExport,
    startExport,
    cancelExport,
    hasAudio,
    showExportSuccessDialog,
    setShowExportSuccessDialog,
    lastExportedPath,
    setLastExportedPath,
    lastExportArtifacts,
    exportAutoCopyEnabled,
    setExportAutoCopyEnabled,
    sourceVideoFps,
    dialogSegment,
    dialogBackgroundConfig,
    dialogTrimmedDurationSec,
    dialogClipCount,
  };
}
