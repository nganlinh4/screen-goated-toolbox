import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@/lib/ipc";
import { notifyUserError } from "@/lib/userNotifications";
import { cloneBackgroundConfig } from "@/lib/backgroundConfig";
import {
  BackgroundConfig,
  ExportArtifact,
  VideoSegment,
  MousePosition,
  ProjectComposition,
  ProjectCompositionClip,
  WebcamConfig,
} from "@/types/video";
import { getTotalTrimDuration } from "@/lib/trimSegments";
import { materializeNarrationGroupTakes } from "@/lib/narrationGroupTakes";
import { cloneWebcamConfig } from "@/lib/webcam";
import { useSettings } from "@/hooks/useSettings";
import { useExportPreferences } from "@/hooks/useExportPreferences";
import {
  getExportFailureMessage,
  normalizeExportArtifacts,
} from "./exportHookUtils";
import { useExportSourceMetadata } from "@/hooks/useExportSourceMetadata";
import {
  ExportCancellationGeneration,
  startAfterCancellablePreparation,
} from "@/lib/exportCancellation";

const loadVideoExporter = async () =>
  (await import("@/lib/videoExporter")).videoExporter;

const loadCompositionExport = () => import("@/lib/compositionExport");

// ============================================================================
// useExport
// ============================================================================
interface UseExportProps {
  videoRef: React.RefObject<HTMLVideoElement | null>;
  webcamVideoRef: React.RefObject<HTMLVideoElement | null>;
  canvasRef: React.RefObject<HTMLCanvasElement | null>;
  tempCanvasRef: React.RefObject<HTMLCanvasElement | null>;
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
  getLatestComposition?: () => ProjectComposition | null;
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

export function useExport(props: UseExportProps) {
  const { t } = useSettings();
  const [isProcessing, setIsProcessing] = useState(false);
  const exportInFlightRef = useRef(false);
  const exportCancellationRef = useRef(new ExportCancellationGeneration());
  const [exportProgress, setExportProgress] = useState(0);
  const [showExportDialog, setShowExportDialog] = useState(false);
  const [showExportSuccessDialog, setShowExportSuccessDialog] = useState(false);
  const [exportErrorMessage, setExportErrorMessage] = useState("");
  const [lastExportedPath, setLastExportedPath] = useState("");
  const [lastExportArtifacts, setLastExportArtifacts] = useState<
    ExportArtifact[]
  >([]);
  const {
    exportAutoCopyEnabled,
    setExportAutoCopyEnabled,
    exportOptions,
    setExportOptions,
  } = useExportPreferences();
  const [hasCheckedExportCapabilities, setHasCheckedExportCapabilities] =
    useState(false);

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

  const { compositionDialogState, sourceVideoFps } = useExportSourceMetadata({
    showExportDialog,
    isCompositionExport,
    composition: props.composition,
    lastCaptureFps: props.lastCaptureFps,
    resolveClipExportSourcePath: props.resolveClipExportSourcePath,
    resolveSourceVideoPath,
  });

  useEffect(() => {
    let cancelled = false;
    void loadVideoExporter()
      .then((exporter) => exporter.getExportCapabilities())
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
    (
      videoEl: HTMLVideoElement,
      canvasEl: HTMLCanvasElement,
      segment: VideoSegment,
      sourceVideoPath: string,
      compositionOverride?: ProjectComposition | null,
    ) => ({
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
      audioFilePath:
        segment.deviceAudioAvailable === false
          ? ""
          : props.audioFilePath || sourceVideoPath,
      micAudioFilePath: props.micAudioFilePath || "",
      webcamVideoFilePath: props.webcamVideoFilePath || "",
      videoFilePath: sourceVideoPath,
      audioSegments: (compositionOverride ?? props.composition)?.audioSegments,
      audioTrackVolumePoints: (compositionOverride ?? props.composition)?.audioTrackVolumePoints,
      narrationSegments: materializeNarrationGroupTakes(
        (compositionOverride ?? props.composition)?.narrationSegments,
      ),
      narrationTrackVolumePoints: (compositionOverride ?? props.composition)?.narrationTrackVolumePoints,
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
      props.composition,
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
      void loadVideoExporter()
        .then((exporter) =>
          exporter.primeExportPreparation(
            buildPrimeArgs(videoEl, canvasEl, segment, sourceVideoPath),
          ),
        )
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
      void loadVideoExporter()
        .then((exporter) =>
          exporter.primeExportPreparation(
            buildPrimeArgs(videoEl, canvasEl, segment, sourceVideoPath),
          ),
        )
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

  const startExport = useCallback(async () => {
    if (exportInFlightRef.current || isProcessing) {
      return;
    }
    const latestComposition = props.getLatestComposition?.() ?? props.composition;
    const useBatchExport =
      !!latestComposition &&
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
    const exportGeneration = exportCancellationRef.current.begin();

    try {
      exportInFlightRef.current = true;
      setShowExportDialog(false);
      setIsProcessing(true);
      setExportErrorMessage("");
      setLastExportArtifacts([]);
      await new Promise<void>((resolve) =>
        requestAnimationFrame(() => resolve()),
      );
      if (!exportCancellationRef.current.isCurrent(exportGeneration)) return;

      const exportResult = useBatchExport && latestComposition
        ? await startAfterCancellablePreparation(
            exportCancellationRef.current,
            exportGeneration,
            loadCompositionExport,
            (compositionExport) =>
              compositionExport.exportCompositionAndDownload({
                composition: latestComposition,
                exportOptions,
                resolveClipSourcePath: props.resolveClipExportSourcePath,
                resolveClipMicAudioPath: props.resolveClipExportMicAudioPath,
                resolveClipWebcamPath: props.resolveClipExportWebcamPath,
                isCancelled: () =>
                  !exportCancellationRef.current.isCurrent(exportGeneration),
              }),
          )
        : await startAfterCancellablePreparation(
            exportCancellationRef.current,
            exportGeneration,
            loadVideoExporter,
            (exporter) => exporter.exportAndDownload({
              ...buildPrimeArgs(
                props.videoRef.current!,
                props.canvasRef.current!,
                props.segment!,
                sourceVideoPath,
                latestComposition,
              ),
              format: exportOptions.format || "mp4",
              onProgress: setExportProgress,
              isCancelled: () =>
                !exportCancellationRef.current.isCurrent(exportGeneration),
            }),
          );
      if (exportResult.cancelled) return;
      const res = exportResult.value;
      const artifacts = normalizeExportArtifacts(res);
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
          }).catch((error) => notifyUserError("copyMediaFailed", error));
        }
      }
    } catch (error) {
      if (!exportCancellationRef.current.isCurrent(exportGeneration)) return;
      console.error("[Export] Error:", error);
      setExportErrorMessage(getExportFailureMessage(error, {
        diskFull: t.exportDiskFull,
        alreadyRunning: t.exportAlreadyRunning,
        unknown: t.exportUnknownFailure,
      }));
    } finally {
      if (exportCancellationRef.current.isCurrent(exportGeneration)) {
        exportInFlightRef.current = false;
        setIsProcessing(false);
        setExportProgress(0);
      }
    }
  }, [
    exportAutoCopyEnabled,
    exportOptions,
    isCompositionExport,
    isProcessing,
    props,
    resolveSourceVideoPath,
    t.exportAlreadyRunning,
    t.exportDiskFull,
    t.exportUnknownFailure,
  ]);

  const cancelExport = useCallback(() => {
    exportCancellationRef.current.cancel();
    exportInFlightRef.current = false;
    void loadVideoExporter()
      .then((exporter) => exporter.cancel())
      .catch((error) => console.error("[Export] Cancel failed:", error));
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
    exportErrorMessage,
    setExportErrorMessage,
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
