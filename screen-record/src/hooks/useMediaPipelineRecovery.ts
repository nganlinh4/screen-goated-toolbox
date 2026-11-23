import {
  useEffect,
  useRef,
  type Dispatch,
  type MutableRefObject,
  type SetStateAction,
} from "react";
import type { Project } from "@/types/video";
import { getMediaServerUrl } from "@/lib/mediaServer";
import type { VideoController } from "@/lib/videoController";

interface UseMediaPipelineRecoveryOptions {
  currentProjectDataRef: MutableRefObject<Project | null>;
  currentRawMicAudioPath: string;
  currentRawVideoPath: string;
  currentRawWebcamVideoPath: string;
  currentTime: number;
  isPlaying: boolean;
  projectId: string | null;
  seek: (time: number) => void;
  segmentDeviceAudioAvailable: boolean | undefined;
  setCurrentAudio: Dispatch<SetStateAction<string | null>>;
  setCurrentMicAudio: Dispatch<SetStateAction<string | null>>;
  setCurrentVideo: Dispatch<SetStateAction<string | null>>;
  setCurrentWebcamVideo: Dispatch<SetStateAction<string | null>>;
  setPreviewAudioResetKey: Dispatch<SetStateAction<number>>;
  videoControllerRef: MutableRefObject<VideoController | undefined>;
}

const revokePreviousBlobUrl = (
  previous: string | null,
  nextUrl: string,
) => {
  if (previous?.startsWith("blob:") && previous !== nextUrl) {
    URL.revokeObjectURL(previous);
  }
  return nextUrl;
};

export function useMediaPipelineRecovery(options: UseMediaPipelineRecoveryOptions) {
  // The reset listener is registered once; it reads the latest playback state
  // and media paths here instead of re-subscribing on every playback frame.
  const latestRef = useRef(options);
  latestRef.current = options;
  const mediaRecoveryInFlightRef = useRef(false);

  useEffect(() => {
    const handleMediaPipelineReset = (event: Event) => {
      const {
        currentProjectDataRef,
        currentRawMicAudioPath,
        currentRawVideoPath,
        currentRawWebcamVideoPath,
        currentTime,
        isPlaying,
        projectId,
        seek,
        segmentDeviceAudioAvailable,
        setCurrentAudio,
        setCurrentMicAudio,
        setCurrentVideo,
        setCurrentWebcamVideo,
        setPreviewAudioResetKey,
        videoControllerRef,
      } = latestRef.current;
      const detail = (event as CustomEvent<{ reason?: string; delayMs?: number }>).detail;
      if (mediaRecoveryInFlightRef.current) return;
      if (!projectId) {
        console.log("[ScreenRecord][MediaReset] project reload skipped: no active project");
        return;
      }
      mediaRecoveryInFlightRef.current = true;
      const resumeTime = currentTime;
      const shouldResume = isPlaying;
      console.log(
        `[ScreenRecord][MediaReset] project reload start project=${projectId} `
        + `reason=${detail?.reason ?? "unknown"} delay=${detail?.delayMs ?? "unknown"}ms `
        + `t=${resumeTime.toFixed(3)} playing=${shouldResume}`,
      );
      void (async () => {
        try {
          const project = currentProjectDataRef.current;
          if (!project || project.id !== projectId) return;
          const controller = videoControllerRef.current;
          let loadVersion = controller?.videoLoadVersion;
          const isCurrent = () => currentProjectDataRef.current?.id === projectId &&
            videoControllerRef.current === controller && controller?.videoLoadVersion === loadVersion;
          let nextVideoUrl: string | undefined;
          if (currentRawVideoPath) {
            const videoUrl = await getMediaServerUrl(currentRawVideoPath);
            if (!isCurrent()) return;
            const pending = controller?.loadVideo({
              videoUrl,
              initialTime: resumeTime,
              debugLabel: "media-reset",
            });
            loadVersion = controller?.videoLoadVersion;
            nextVideoUrl = await pending;
          } else if (project.videoBlob) {
            const pending = controller?.loadVideo({
              videoBlob: project.videoBlob,
              initialTime: resumeTime,
              debugLabel: "media-reset",
            });
            loadVersion = controller?.videoLoadVersion;
            nextVideoUrl = await pending;
          }
          if (!isCurrent()) return;
          if (nextVideoUrl) {
            setCurrentVideo((previous) => revokePreviousBlobUrl(previous, nextVideoUrl));
          }
          let nextAudioUrl: string | undefined;
          if (currentRawVideoPath && segmentDeviceAudioAvailable !== false) {
            const audioUrl = await getMediaServerUrl(currentRawVideoPath);
            if (!isCurrent()) return;
            nextAudioUrl = await controller?.loadDeviceAudio({ audioUrl });
          } else if (project.audioBlob) {
            nextAudioUrl = await videoControllerRef.current?.loadDeviceAudio({
              audioBlob: project.audioBlob,
            });
          }
          if (!isCurrent()) return;
          if (nextAudioUrl) {
            setCurrentAudio((previous) => revokePreviousBlobUrl(previous, nextAudioUrl));
          }
          let nextMicAudioUrl: string | undefined;
          if (currentRawMicAudioPath) {
            const audioUrl = await getMediaServerUrl(currentRawMicAudioPath);
            if (!isCurrent()) return;
            nextMicAudioUrl = await controller?.loadMicAudio({ audioUrl });
          } else if (project.micAudioBlob) {
            nextMicAudioUrl = await videoControllerRef.current?.loadMicAudio({
              audioBlob: project.micAudioBlob,
            });
          }
          if (!isCurrent()) return;
          if (nextMicAudioUrl) {
            setCurrentMicAudio((previous) => revokePreviousBlobUrl(previous, nextMicAudioUrl));
          }
          let nextWebcamUrl: string | undefined;
          if (currentRawWebcamVideoPath) {
            const videoUrl = await getMediaServerUrl(currentRawWebcamVideoPath);
            if (!isCurrent()) return;
            nextWebcamUrl = await controller?.loadWebcamVideo({ videoUrl });
          } else if (project.webcamBlob) {
            nextWebcamUrl = await videoControllerRef.current?.loadWebcamVideo({
              videoBlob: project.webcamBlob,
            });
          }
          if (!isCurrent()) return;
          if (nextWebcamUrl) {
            setCurrentWebcamVideo((previous) => revokePreviousBlobUrl(previous, nextWebcamUrl));
          }
          setPreviewAudioResetKey((key) => key + 1);
          requestAnimationFrame(() => {
            if (!isCurrent()) return;
            seek(resumeTime);
            if (shouldResume) {
              window.setTimeout(() => { if (isCurrent()) controller?.play(); }, 250);
            }
          });
          console.log("[ScreenRecord][MediaReset] project reload complete");
        } catch (error) {
          if (!(error instanceof DOMException && error.name === "AbortError")) {
            console.warn("[ScreenRecord][MediaReset] project reload failed", error);
          }
        } finally {
          window.setTimeout(() => {
            mediaRecoveryInFlightRef.current = false;
          }, 5000);
        }
      })();
    };
    window.addEventListener("sr-reset-media-pipeline", handleMediaPipelineReset);
    return () => {
      window.removeEventListener("sr-reset-media-pipeline", handleMediaPipelineReset);
    };
  }, []);
}
