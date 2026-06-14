import { createVideoController } from "@/lib/videoController";
import { Project } from "@/types/video";
import { getMediaServerUrl } from "@/lib/mediaServer";

type VideoController = ReturnType<typeof createVideoController>;

export interface LoadProjectVideoParams {
  controller: VideoController | undefined;
  project: Project;
  isTimelineOnlyProject: boolean;
  rawVideoPath: string;
}

export interface LoadProjectAudioParams {
  controller: VideoController | undefined;
  project: Project;
  rawVideoPath: string;
  rawMicAudioPath: string;
  rawWebcamVideoPath: string;
  videoObjectUrl: string | undefined;
}

export interface LoadProjectAudioResult {
  audioObjectUrl: string | undefined;
  micAudioObjectUrl: string | undefined;
  webcamVideoObjectUrl: string | undefined;
}

/**
 * Load the main video (or its blob) into the controller and return the object
 * URL. Extracted verbatim from the project load path. Kept separate from audio
 * loading so the caller can re-check the load-request sequence guard between
 * the two phases, exactly as the original load path did.
 */
export async function loadProjectVideo({
  controller,
  project,
  isTimelineOnlyProject,
  rawVideoPath,
}: LoadProjectVideoParams): Promise<string | undefined> {
  let videoObjectUrl: string | undefined;
  if (!isTimelineOnlyProject && rawVideoPath) {
    const mediaUrl = await getMediaServerUrl(rawVideoPath);
    videoObjectUrl = await controller?.loadVideo({
      videoUrl: mediaUrl,
      initialTime: project.segment.trimStart,
      debugLabel: "project-load",
    });
  } else if (!isTimelineOnlyProject && project.videoBlob) {
    videoObjectUrl = await controller?.loadVideo({
      videoBlob: project.videoBlob,
      initialTime: project.segment.trimStart,
      debugLabel: "project-load",
    });
  }
  return videoObjectUrl;
}

/**
 * Load device audio, mic audio, and webcam video into the controller and
 * return the resulting object URLs. Extracted verbatim from the project load
 * path; pure media loading with no React state side effects.
 */
export async function loadProjectAudioMedia({
  controller,
  project,
  rawVideoPath,
  rawMicAudioPath,
  rawWebcamVideoPath,
  videoObjectUrl,
}: LoadProjectAudioParams): Promise<LoadProjectAudioResult> {
  let audioObjectUrl: string | undefined;
  let micAudioObjectUrl: string | undefined;
  let webcamVideoObjectUrl: string | undefined;
  if (rawVideoPath && project.segment.deviceAudioAvailable !== false) {
    const mediaUrl = await getMediaServerUrl(rawVideoPath);
    audioObjectUrl = await controller?.loadDeviceAudio({
      audioUrl: mediaUrl,
    });
  } else if (project.audioBlob) {
    audioObjectUrl = await controller?.loadDeviceAudio({
      audioBlob: project.audioBlob,
    });
  } else if (videoObjectUrl) {
    audioObjectUrl = await controller?.loadDeviceAudio({
      audioUrl: videoObjectUrl,
    });
  }
  if (rawMicAudioPath) {
    const mediaUrl = await getMediaServerUrl(rawMicAudioPath);
    micAudioObjectUrl = await controller?.loadMicAudio({
      audioUrl: mediaUrl,
    });
  } else if (project.micAudioBlob) {
    micAudioObjectUrl = await controller?.loadMicAudio({
      audioBlob: project.micAudioBlob,
    });
  }
  if (rawWebcamVideoPath) {
    const mediaUrl = await getMediaServerUrl(rawWebcamVideoPath);
    webcamVideoObjectUrl = await controller?.loadWebcamVideo({
      videoUrl: mediaUrl,
    });
  } else if (project.webcamBlob) {
    webcamVideoObjectUrl = await controller?.loadWebcamVideo({
      videoBlob: project.webcamBlob,
    });
  }

  return {
    audioObjectUrl,
    micAudioObjectUrl,
    webcamVideoObjectUrl,
  };
}
