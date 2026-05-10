import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@/lib/ipc";
import { startAudioTrackDownload } from "@/lib/audioDownload";
import type {
  AudioDownloadFormat,
  AudioDownloadResult,
  AudioDownloadTrackKind,
  ProjectComposition,
  ProjectCompositionClip,
  VideoSegment,
} from "@/types/video";

interface UseAudioDownloadProps {
  videoRef: React.RefObject<HTMLVideoElement | null>;
  segment: VideoSegment | null;
  sourceVideoPath: string;
  micAudioPath: string;
  composition: ProjectComposition | null;
  getLatestComposition?: () => ProjectComposition | null;
  resolveClipExportSourcePath: (clip: ProjectCompositionClip) => Promise<string>;
  resolveClipExportMicAudioPath: (clip: ProjectCompositionClip) => Promise<string>;
}

export interface PendingAudioDownload {
  trackKind: AudioDownloadTrackKind;
  trackLabel: string;
}

export function useAudioDownload(props: UseAudioDownloadProps) {
  const [showDialog, setShowDialog] = useState(false);
  const [pendingTrack, setPendingTrack] = useState<PendingAudioDownload | null>(null);
  const [format, setFormat] = useState<AudioDownloadFormat>("mp3");
  const [outputDir, setOutputDir] = useState("");
  const [isProcessing, setIsProcessing] = useState(false);
  const [result, setResult] = useState<AudioDownloadResult | null>(null);
  const [showResultDialog, setShowResultDialog] = useState(false);
  const inFlightRef = useRef(false);

  useEffect(() => {
    if (!showDialog || outputDir) return;
    invoke<string>("get_default_export_dir")
      .then((dir) => {
        if (dir) setOutputDir(dir);
      })
      .catch((error) => console.error("[AudioDownload] default dir failed:", error));
  }, [showDialog, outputDir]);

  const openAudioDownloadDialog = useCallback((trackKind: AudioDownloadTrackKind, trackLabel: string) => {
    setPendingTrack({ trackKind, trackLabel });
    setShowDialog(true);
  }, []);

  const startDownload = useCallback(async () => {
    if (!pendingTrack || inFlightRef.current || isProcessing) return;
    const latestComposition = props.getLatestComposition?.() ?? props.composition;
    try {
      inFlightRef.current = true;
      setShowDialog(false);
      setIsProcessing(true);
      const response = await startAudioTrackDownload({
        trackKind: pendingTrack.trackKind,
        trackLabel: pendingTrack.trackLabel,
        format,
        outputDir,
        segment: props.segment,
        sourceVideoPath: props.sourceVideoPath,
        micAudioPath: props.micAudioPath,
        videoDuration: props.videoRef.current?.duration || props.segment?.trimEnd || 0,
        composition: latestComposition,
        resolveClipSourcePath: props.resolveClipExportSourcePath,
        resolveClipMicAudioPath: props.resolveClipExportMicAudioPath,
      });
      if (response?.status === "success" && response.path) {
        setResult(response);
        setShowResultDialog(true);
      }
    } catch (error) {
      console.error("[AudioDownload] Error:", error);
    } finally {
      inFlightRef.current = false;
      setIsProcessing(false);
    }
  }, [format, isProcessing, outputDir, pendingTrack, props]);

  const cancelAudioDownload = useCallback(() => {
    inFlightRef.current = false;
    invoke("cancel_export").catch(console.error);
    setIsProcessing(false);
  }, []);

  return {
    showDialog,
    setShowDialog,
    pendingTrack,
    format,
    setFormat,
    outputDir,
    setOutputDir,
    startDownload,
    isProcessing,
    cancelAudioDownload,
    result,
    setResult,
    showResultDialog,
    setShowResultDialog,
    openAudioDownloadDialog,
  };
}
