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

export function useMediaPipelineRecovery({
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
}: UseMediaPipelineRecoveryOptions) {
  const mediaRecoveryInFlightRef = useRef(false);

  useEffect(() => {
    const handleMediaPipelineReset = (event: Event) => {
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
          let nextVideoUrl: string | undefined;
          if (currentRawVideoPath) {
            nextVideoUrl = await videoControllerRef.current?.loadVideo({
              videoUrl: await getMediaServerUrl(currentRawVideoPath),
              initialTime: resumeTime,
              debugLabel: "media-reset",
            });
          } else if (project.videoBlob) {
            nextVideoUrl = await videoControllerRef.current?.loadVideo({
              videoBlob: project.videoBlob,
              initialTime: resumeTime,
              debugLabel: "media-reset",
            });
          }
          if (nextVideoUrl) {
            setCurrentVideo((previous) => revokePreviousBlobUrl(previous, nextVideoUrl));
          }
          let nextAudioUrl: string | undefined;
          if (currentRawVideoPath && segmentDeviceAudioAvailable !== false) {
            nextAudioUrl = await videoControllerRef.current?.loadDeviceAudio({
              audioUrl: await getMediaServerUrl(currentRawVideoPath),
            });
          } else if (project.audioBlob) {
            nextAudioUrl = await videoControllerRef.current?.loadDeviceAudio({
              audioBlob: project.audioBlob,
            });
          }
          if (nextAudioUrl) {
            setCurrentAudio((previous) => revokePreviousBlobUrl(previous, nextAudioUrl));
          }
          let nextMicAudioUrl: string | undefined;
          if (currentRawMicAudioPath) {
            nextMicAudioUrl = await videoControllerRef.current?.loadMicAudio({
              audioUrl: await getMediaServerUrl(currentRawMicAudioPath),
            });
          } else if (project.micAudioBlob) {
            nextMicAudioUrl = await videoControllerRef.current?.loadMicAudio({
              audioBlob: project.micAudioBlob,
            });
          }
          if (nextMicAudioUrl) {
            setCurrentMicAudio((previous) => revokePreviousBlobUrl(previous, nextMicAudioUrl));
          }
          let nextWebcamUrl: string | undefined;
          if (currentRawWebcamVideoPath) {
            nextWebcamUrl = await videoControllerRef.current?.loadWebcamVideo({
              videoUrl: await getMediaServerUrl(currentRawWebcamVideoPath),
            });
          } else if (project.webcamBlob) {
            nextWebcamUrl = await videoControllerRef.current?.loadWebcamVideo({
              videoBlob: project.webcamBlob,
            });
          }
          if (nextWebcamUrl) {
            setCurrentWebcamVideo((previous) => revokePreviousBlobUrl(previous, nextWebcamUrl));
          }
          setPreviewAudioResetKey((key) => key + 1);
          requestAnimationFrame(() => {
            seek(resumeTime);
            if (shouldResume) {
              window.setTimeout(() => videoControllerRef.current?.play(), 250);
            }
          });
          console.log("[ScreenRecord][MediaReset] project reload complete");
        } catch (error) {
          console.warn("[ScreenRecord][MediaReset] project reload failed", error);
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
  }, [
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
  ]);
}
