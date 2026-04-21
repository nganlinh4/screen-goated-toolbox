import { useCallback, useRef, useState } from "react";
import { projectManager } from "@/lib/projectManager";
import { importVideoToManagedMediaFile, getMediaServerUrl } from "@/lib/mediaServer";
import { buildFlatDeviceAudioPoints } from "@/lib/deviceAudio";
import { DEFAULT_BACKGROUND_CONFIG } from "@/lib/appUtils";
import { createSubtitleTrackStateFromSegments } from "@/lib/subtitleTracks";
import type { VideoSegment, Project } from "@/types/video";

export interface UseVideoImportResult {
  isImporting: boolean;
  importVideo: (file: File) => Promise<void>;
}

function buildVideoImportTraceId() {
  return `video-import-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
}

export function useVideoImport(opts: {
  onProjectCreated: (project: Project) => void;
}): UseVideoImportResult {
  const [isImporting, setIsImporting] = useState(false);
  const isImportingRef = useRef(false);

  const importVideo = useCallback(async (file: File) => {
    if (!file.type.startsWith("video/")) return;
    if (isImportingRef.current) return;

    isImportingRef.current = true;
    setIsImporting(true);
    const traceId = buildVideoImportTraceId();
    try {
      // 1. Persist the uploaded source into the app-managed recordings area.
      const { path: rawVideoPath, hasAudio } = await importVideoToManagedMediaFile(
        file,
        file.name,
        traceId,
      );

      // 2. Probe duration + generate thumbnail via temporary video element
      const videoUrl = await getMediaServerUrl(rawVideoPath);
      const { duration, thumbnail } = await probeVideo(videoUrl);

      // 3. Build minimal segment
      const segment: VideoSegment = {
        trimStart: 0,
        trimEnd: duration,
        trimSegments: [{ id: crypto.randomUUID(), startTime: 0, endTime: duration }],
        zoomKeyframes: [],
        textSegments: [],
        ...createSubtitleTrackStateFromSegments([]),
        speedPoints: [
          { time: 0, speed: 1 },
          { time: duration, speed: 1 },
        ],
        deviceAudioPoints: buildFlatDeviceAudioPoints(duration),
        deviceAudioAvailable: hasAudio,
        micAudioAvailable: false,
        webcamAvailable: false,
        useCustomCursor: false,
        keystrokeMode: "off",
        keystrokeEvents: [],
        keyboardVisibilitySegments: [],
        keyboardMouseVisibilitySegments: [],
      };

      // 4. Save project
      const name = file.name.replace(/\.[^.]+$/, "") || "Imported Video";
      const project = await projectManager.saveProject({
        name,
        segment,
        backgroundConfig: { ...DEFAULT_BACKGROUND_CONFIG },
        mousePositions: [],
        thumbnail,
        recordingMode: "imported",
        rawVideoPath,
        duration,
      });

      opts.onProjectCreated(project);
    } catch (err) {
      console.error(`[VideoImport:${traceId}] failed`, err);
    } finally {
      isImportingRef.current = false;
      setIsImporting(false);
    }
  }, [opts]);

  return { isImporting, importVideo };
}

function probeVideo(
  url: string,
): Promise<{
  duration: number;
  thumbnail: string | undefined;
}> {
  return new Promise((resolve, reject) => {
    const video = document.createElement("video");
    video.muted = true;
    video.preload = "auto";
    video.crossOrigin = "anonymous";

    const cleanup = () => {
      video.removeAttribute("src");
      video.load();
    };

    video.onloadedmetadata = () => {
      const duration = video.duration;
      if (!isFinite(duration) || duration <= 0) {
        cleanup();
        reject(new Error("Invalid video duration"));
        return;
      }

      // Seek to first frame for thumbnail
      video.currentTime = Math.min(0.5, duration * 0.1);
    };

    video.onseeked = () => {
      let thumbnail: string | undefined;
      try {
        const canvas = document.createElement("canvas");
        canvas.width = Math.min(video.videoWidth, 640);
        canvas.height = Math.round(canvas.width * (video.videoHeight / video.videoWidth));
        const ctx = canvas.getContext("2d");
        if (ctx) {
          ctx.drawImage(video, 0, 0, canvas.width, canvas.height);
          thumbnail = canvas.toDataURL("image/jpeg", 0.7);
        }
      } catch { /* thumbnail generation failed, proceed without */ }

      cleanup();
      resolve({
        duration: video.duration,
        thumbnail,
      });
    };

    video.onerror = () => {
      cleanup();
      reject(new Error("Failed to load video"));
    };

    video.src = url;
  });
}
