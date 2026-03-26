import { useState, useRef, useEffect, useCallback } from "react";
import { createVideoController } from "@/lib/videoController";
import { cloneBackgroundConfig } from "@/lib/backgroundConfig";
import { projectManager } from "@/lib/projectManager";
import {
  BackgroundConfig,
  VideoSegment,
  MousePosition,
  Project,
  RecordingMode,
  WebcamConfig,
} from "@/types/video";
import {
  clampVisibilitySegmentsToDuration,
} from "@/lib/cursorHiding";
import { normalizeSegmentTrimData } from "@/lib/trimSegments";
import {
  ensureKeystrokeVisibilitySegments,
  filterKeystrokeEventsByMode,
  rebuildKeystrokeVisibilitySegmentsForMode,
} from "@/lib/keystrokeVisibility";
import {
  normalizeDeviceAudioPoints,
} from "@/lib/deviceAudio";
import {
  normalizeMicAudioPoints,
} from "@/lib/micAudio";
import { cloneWebcamConfig } from "@/lib/webcam";
import {
  normalizeWebcamVisibilitySegments,
} from "@/lib/webcamVisibility";
import { getMediaServerUrl, writeBlobToTempMediaFile } from "@/lib/mediaServer";
import {
  normalizeCropRect,
  normalizeTrackDelaySec,
  summarizeLoadedBackground,
  DEFAULT_KEYSTROKE_DELAY_SEC,
  PROJECT_LOAD_DEBUG,
  PROJECT_SWITCH_DEBUG,
} from "./videoStatePreferences";

// ============================================================================
// useProjects
// ============================================================================
interface UseProjectsProps {
  videoControllerRef: React.MutableRefObject<
    ReturnType<typeof createVideoController> | undefined
  >;
  setCurrentVideo: (url: string | null) => void;
  setCurrentAudio: (url: string | null) => void;
  setCurrentMicAudio: (url: string | null) => void;
  setCurrentWebcamVideo: (url: string | null) => void;
  setSegment: (segment: VideoSegment | null) => void;
  setBackgroundConfig: React.Dispatch<React.SetStateAction<BackgroundConfig>>;
  setWebcamConfig?: React.Dispatch<React.SetStateAction<WebcamConfig>>;
  applyLoadedBackgroundConfig?: (backgroundConfig: BackgroundConfig) => void;
  setMousePositions: (positions: MousePosition[]) => void;
  setThumbnails: (thumbnails: string[]) => void;
  setCurrentRecordingMode?: (mode: RecordingMode) => void;
  setCurrentRawVideoPath?: (path: string) => void;
  onProjectLoaded?: (project: Project) => void;
  currentVideo: string | null;
  currentAudio: string | null;
  currentMicAudio: string | null;
  currentWebcamVideo?: string | null;
}

export function useProjects(props: UseProjectsProps) {
  const [projects, setProjects] = useState<
    Omit<Project, "videoBlob" | "audioBlob" | "micAudioBlob" | "webcamBlob">[]
  >([]);
  const [showProjectsDialog, setShowProjectsDialog] = useState(false);
  const [currentProjectId, setCurrentProjectId] = useState<string | null>(null);
  const loadRequestSeqRef = useRef(0);
  const logProjectLoad = (event: string, data?: Record<string, unknown>) => {
    if (!PROJECT_LOAD_DEBUG) return;
    const ts = new Date().toISOString();
    console.log(`[ProjectLoad][${ts}] ${event}`, data || {});
  };
  const logProjectSwitch = (event: string, data?: Record<string, unknown>) => {
    if (!PROJECT_SWITCH_DEBUG) return;
    console.warn(`[ProjectSwitch] ${JSON.stringify({ event, ...data })}`);
  };

  const loadProjects = useCallback(async () => {
    const projects = await projectManager.getProjects();
    setProjects(projects);
  }, []);

  const handleLoadProject = useCallback(
    async (projectId: string) => {
      const loadRequestSeq = ++loadRequestSeqRef.current;
      logProjectLoad("load:start", { projectId });
      const project = await projectManager.loadProject(projectId);
      if (!project || loadRequestSeq !== loadRequestSeqRef.current) {
        logProjectLoad("load:missing", { projectId });
        return;
      }
      logProjectLoad("load:fetched", {
        projectId,
        canvasMode: project.backgroundConfig?.canvasMode,
        canvasWidth: project.backgroundConfig?.canvasWidth,
        canvasHeight: project.backgroundConfig?.canvasHeight,
      });
      logProjectSwitch("load:fetched", {
        projectId,
        currentProjectIdBefore: currentProjectId,
        fetchedBackground: summarizeLoadedBackground(project.backgroundConfig),
        trimEnd: project.segment?.trimEnd ?? null,
      });

      const previousVideoUrl = props.currentVideo;
      const previousAudioUrl = props.currentAudio;
      const previousMicAudioUrl = props.currentMicAudio;
      const previousWebcamVideoUrl = props.currentWebcamVideo;

      // Restore rawVideoPath for old projects that only have a blob.
      // Writes the blob to disk via the media server POST endpoint (binary, no JSON overhead).
      let rawVideoPath = project.rawVideoPath ?? "";
      if (!rawVideoPath && project.videoBlob && project.videoBlob.size > 0) {
        try {
          rawVideoPath = await writeBlobToTempMediaFile(project.videoBlob);
          if (rawVideoPath) {
            // Persist so this migration only happens once.
            await projectManager.updateProject(projectId, {
              ...project,
              rawVideoPath,
            });
          }
        } catch (e) {
          console.error("[ProjectLoad] Failed to restore rawVideoPath:", e);
        }
      }
      let rawMicAudioPath = project.rawMicAudioPath ?? "";
      if (!rawMicAudioPath && project.micAudioBlob && project.micAudioBlob.size > 0) {
        try {
          rawMicAudioPath = await writeBlobToTempMediaFile(project.micAudioBlob);
          if (rawMicAudioPath) {
            await projectManager.updateProject(projectId, {
              ...project,
              rawVideoPath,
              rawMicAudioPath,
            });
          }
        } catch (e) {
          console.error("[ProjectLoad] Failed to restore rawMicAudioPath:", e);
        }
      }
      let rawWebcamVideoPath = project.rawWebcamVideoPath ?? "";
      if (!rawWebcamVideoPath && project.webcamBlob && project.webcamBlob.size > 0) {
        try {
          rawWebcamVideoPath = await writeBlobToTempMediaFile(project.webcamBlob);
          if (rawWebcamVideoPath) {
            await projectManager.updateProject(projectId, {
              ...project,
              rawVideoPath,
              rawMicAudioPath,
              rawWebcamVideoPath,
            });
          }
        } catch (e) {
          console.error("[ProjectLoad] Failed to restore rawWebcamVideoPath:", e);
        }
      }
      if (loadRequestSeq !== loadRequestSeqRef.current) return;

      let videoObjectUrl: string | undefined;
      if (rawVideoPath) {
        const mediaUrl = await getMediaServerUrl(rawVideoPath);
        videoObjectUrl = await props.videoControllerRef.current?.loadVideo({
          videoUrl: mediaUrl,
          debugLabel: "project-load",
        });
      } else if (project.videoBlob) {
        videoObjectUrl = await props.videoControllerRef.current?.loadVideo({
          videoBlob: project.videoBlob,
          debugLabel: "project-load",
        });
      }
      if (loadRequestSeq !== loadRequestSeqRef.current) return;

      let audioObjectUrl: string | undefined;
      let micAudioObjectUrl: string | undefined;
      let webcamVideoObjectUrl: string | undefined;
      if (rawVideoPath) {
        const mediaUrl = await getMediaServerUrl(rawVideoPath);
        audioObjectUrl = await props.videoControllerRef.current?.loadDeviceAudio(
          {
            audioUrl: mediaUrl,
          },
        );
      } else if (project.audioBlob) {
        audioObjectUrl = await props.videoControllerRef.current?.loadDeviceAudio({
          audioBlob: project.audioBlob,
        });
      } else if (videoObjectUrl) {
        audioObjectUrl = await props.videoControllerRef.current?.loadDeviceAudio({
          audioUrl: videoObjectUrl,
        });
      }
      if (rawMicAudioPath) {
        const mediaUrl = await getMediaServerUrl(rawMicAudioPath);
        micAudioObjectUrl = await props.videoControllerRef.current?.loadMicAudio(
          {
            audioUrl: mediaUrl,
          },
        );
      } else if (project.micAudioBlob) {
        micAudioObjectUrl = await props.videoControllerRef.current?.loadMicAudio(
          {
            audioBlob: project.micAudioBlob,
          },
        );
      }
      if (rawWebcamVideoPath) {
        const mediaUrl = await getMediaServerUrl(rawWebcamVideoPath);
        webcamVideoObjectUrl =
          await props.videoControllerRef.current?.loadWebcamVideo({
            videoUrl: mediaUrl,
          });
      } else if (project.webcamBlob) {
        webcamVideoObjectUrl =
          await props.videoControllerRef.current?.loadWebcamVideo({
            videoBlob: project.webcamBlob,
          });
      }
      if (loadRequestSeq !== loadRequestSeqRef.current) return;

      const videoDuration = props.videoControllerRef.current?.duration || 0;
      let correctedSegment = { ...project.segment };
      const hasExplicitPointerSegments = Array.isArray(
        correctedSegment.cursorVisibilitySegments,
      );
      if (
        correctedSegment.trimEnd === 0 ||
        correctedSegment.trimEnd > videoDuration
      ) {
        correctedSegment.trimEnd = videoDuration;
      }
      correctedSegment = normalizeSegmentTrimData(
        correctedSegment,
        videoDuration,
      );
      if (typeof correctedSegment.useCustomCursor !== "boolean") {
        correctedSegment.useCustomCursor =
          project.recordingMode === "withCursor" ? false : true;
      }
      correctedSegment.crop = normalizeCropRect(correctedSegment.crop);
      correctedSegment.deviceAudioPoints = normalizeDeviceAudioPoints(
        correctedSegment.deviceAudioPoints,
        videoDuration,
        project.backgroundConfig.volume,
      );
      correctedSegment.micAudioPoints = normalizeMicAudioPoints(
        correctedSegment.micAudioPoints,
        videoDuration,
      );
      correctedSegment.micAudioOffsetSec = normalizeTrackDelaySec(
        correctedSegment.micAudioOffsetSec,
      );
      correctedSegment.deviceAudioAvailable =
        correctedSegment.deviceAudioAvailable !== false;
      correctedSegment.micAudioAvailable =
        typeof correctedSegment.micAudioAvailable === "boolean"
          ? correctedSegment.micAudioAvailable
          : Boolean(project.rawMicAudioPath || project.micAudioBlob || micAudioObjectUrl);
      correctedSegment.webcamAvailable =
        typeof correctedSegment.webcamAvailable === "boolean"
          ? correctedSegment.webcamAvailable
          : Boolean(rawWebcamVideoPath || project.webcamBlob || webcamVideoObjectUrl);
      correctedSegment.webcamOffsetSec = normalizeTrackDelaySec(
        correctedSegment.webcamOffsetSec,
      );
      correctedSegment.webcamVisibilitySegments = normalizeWebcamVisibilitySegments(
        correctedSegment.webcamVisibilitySegments,
        videoDuration,
        correctedSegment.webcamAvailable !== false,
      );
      correctedSegment.cursorVisibilitySegments =
        clampVisibilitySegmentsToDuration(
          correctedSegment.cursorVisibilitySegments,
          videoDuration,
        );
      correctedSegment.keyboardVisibilitySegments =
        clampVisibilitySegmentsToDuration(
          correctedSegment.keyboardVisibilitySegments,
          videoDuration,
        );
      correctedSegment.keyboardMouseVisibilitySegments =
        clampVisibilitySegmentsToDuration(
          correctedSegment.keyboardMouseVisibilitySegments,
          videoDuration,
        );
      // Materialize pointer segments for backward-compat (old projects have undefined)
      if (!hasExplicitPointerSegments) {
        correctedSegment.cursorVisibilitySegments = [
          {
            id: crypto.randomUUID(),
            startTime: 0,
            endTime: videoDuration,
          },
        ];
      }
      if (
        !correctedSegment.speedPoints ||
        correctedSegment.speedPoints.length === 0
      ) {
        correctedSegment.speedPoints = [
          { time: 0, speed: 1 },
          { time: videoDuration, speed: 1 },
        ];
      }
      if (!correctedSegment.keystrokeMode) {
        correctedSegment.keystrokeMode = "off";
      }
      if (!Array.isArray(correctedSegment.keystrokeEvents)) {
        correctedSegment.keystrokeEvents = [];
      }
      if (
        typeof correctedSegment.keystrokeDelaySec !== "number" ||
        Number.isNaN(correctedSegment.keystrokeDelaySec)
      ) {
        correctedSegment.keystrokeDelaySec = DEFAULT_KEYSTROKE_DELAY_SEC;
      } else {
        correctedSegment.keystrokeDelaySec = Math.max(
          -1,
          Math.min(1, correctedSegment.keystrokeDelaySec),
        );
      }
      const overlay = correctedSegment.keystrokeOverlay;
      correctedSegment.keystrokeOverlay = {
        x:
          typeof overlay?.x === "number"
            ? Math.max(0, Math.min(100, overlay.x))
            : 50,
        y:
          typeof overlay?.y === "number"
            ? Math.max(0, Math.min(100, overlay.y))
            : 100,
        scale:
          typeof overlay?.scale === "number" && Number.isFinite(overlay.scale)
            ? Math.max(0.45, Math.min(2.4, overlay.scale))
            : 1,
      };
      correctedSegment = ensureKeystrokeVisibilitySegments(
        correctedSegment,
        videoDuration,
      );
      const loadedMode = correctedSegment.keystrokeMode ?? "off";
      if (loadedMode === "keyboard" || loadedMode === "keyboardMouse") {
        const modeEvents = filterKeystrokeEventsByMode(
          correctedSegment.keystrokeEvents ?? [],
          loadedMode,
        );
        const modeSegments =
          loadedMode === "keyboard"
            ? (correctedSegment.keyboardVisibilitySegments ?? [])
            : (correctedSegment.keyboardMouseVisibilitySegments ?? []);
        if (modeSegments.length === 0 && modeEvents.length > 0) {
          correctedSegment = rebuildKeystrokeVisibilitySegmentsForMode(
            correctedSegment,
            loadedMode,
            videoDuration,
          );
        }
      }

      // Draw the first frame on the canvas immediately (before React state updates)
      // so the canvas has content when the projects overlay fades out.
      props.videoControllerRef.current?.renderImmediate({
        segment: correctedSegment,
        backgroundConfig: cloneBackgroundConfig(project.backgroundConfig),
        webcamConfig: cloneWebcamConfig(project.webcamConfig),
        mousePositions: project.mousePositions,
      });

      setCurrentProjectId(projectId);
      props.setThumbnails([]);
      if (videoObjectUrl) {
        props.setCurrentVideo(videoObjectUrl);
        if (
          previousVideoUrl?.startsWith("blob:") &&
          previousVideoUrl !== videoObjectUrl
        ) {
          URL.revokeObjectURL(previousVideoUrl);
        }
      }
      if (audioObjectUrl) {
        props.setCurrentAudio(audioObjectUrl);
        if (
          previousAudioUrl?.startsWith("blob:") &&
          previousAudioUrl !== audioObjectUrl
        ) {
          URL.revokeObjectURL(previousAudioUrl);
        }
      } else {
        props.setCurrentAudio(null);
        if (previousAudioUrl?.startsWith("blob:")) {
          URL.revokeObjectURL(previousAudioUrl);
        }
      }
      if (micAudioObjectUrl) {
        props.setCurrentMicAudio(micAudioObjectUrl);
        if (
          previousMicAudioUrl?.startsWith("blob:") &&
          previousMicAudioUrl !== micAudioObjectUrl
        ) {
          URL.revokeObjectURL(previousMicAudioUrl);
        }
      } else {
        props.setCurrentMicAudio(null);
        if (previousMicAudioUrl?.startsWith("blob:")) {
          URL.revokeObjectURL(previousMicAudioUrl);
        }
      }
      if (webcamVideoObjectUrl) {
        props.setCurrentWebcamVideo(webcamVideoObjectUrl);
        if (
          previousWebcamVideoUrl?.startsWith("blob:") &&
          previousWebcamVideoUrl !== webcamVideoObjectUrl
        ) {
          URL.revokeObjectURL(previousWebcamVideoUrl);
        }
      } else {
        props.setCurrentWebcamVideo(null);
        if (previousWebcamVideoUrl?.startsWith("blob:")) {
          URL.revokeObjectURL(previousWebcamVideoUrl);
        }
      }
      props.setSegment(correctedSegment);
      const loadedBackground = cloneBackgroundConfig(project.backgroundConfig);
      props.setWebcamConfig?.(cloneWebcamConfig(project.webcamConfig));
      if (props.applyLoadedBackgroundConfig) {
        props.applyLoadedBackgroundConfig(loadedBackground);
      } else {
        props.setBackgroundConfig(loadedBackground);
      }
      props.setMousePositions(project.mousePositions);
      props.setCurrentRecordingMode?.(project.recordingMode ?? "withoutCursor");
      props.setCurrentRawVideoPath?.(rawVideoPath);
      logProjectSwitch("load:apply-state", {
        projectId,
        currentProjectIdAfterSet: projectId,
        appliedBackground: summarizeLoadedBackground(project.backgroundConfig),
        appliedTrimEnd: correctedSegment.trimEnd,
      });
      props.onProjectLoaded?.({
        ...project,
        rawVideoPath,
        rawMicAudioPath,
        rawWebcamVideoPath,
        segment: correctedSegment,
      });
      logProjectLoad("load:applied", {
        projectId,
        canvasMode: project.backgroundConfig?.canvasMode,
        canvasWidth: project.backgroundConfig?.canvasWidth,
        canvasHeight: project.backgroundConfig?.canvasHeight,
      });
      // Ensure keyboard focus returns to the document after the Projects overlay
      // animates out (clone removal can leave focus in limbo → spacebar ignored).
      requestAnimationFrame(() => document.body.focus());
    },
    [props],
  );

  useEffect(() => {
    loadProjects();
  }, [loadProjects]);

  return {
    projects,
    showProjectsDialog,
    setShowProjectsDialog,
    currentProjectId,
    setCurrentProjectId,
    loadProjects,
    handleLoadProject,
  };
}
