import { useState, useRef, useEffect, useCallback } from "react";
import { createVideoController } from "@/lib/videoController";
import { cloneBackgroundConfig } from "@/lib/backgroundConfig";
import { projectManager } from "@/lib/projectManager";
import { sortProjectsByDisplayOrder } from "@/lib/projectStorage";
import {
  BackgroundConfig,
  VideoSegment,
  MousePosition,
  Project,
  RecordingMode,
  WebcamConfig,
} from "@/types/video";
import { cloneWebcamConfig } from "@/lib/webcam";
import {
  getMediaServerUrl,
  createAudioPlaceholderVideo,
  importVideoToManagedMediaFile,
  isManagedImportedVideoPath,
  writeBlobToTempMediaFile,
} from "@/lib/mediaServer";
import { getVisibleSubtitleSegments } from "@/lib/subtitleTracks";
import {
  summarizeLoadedBackground,
  PROJECT_LOAD_DEBUG,
  PROJECT_SWITCH_DEBUG,
} from "../videoStatePreferences";
import { restoreRawPath } from "./projectMediaMigration";
import { reportStartupMilestone } from "@/lib/startupTelemetry";
import {
  loadProjectVideo,
  loadProjectAudioMedia,
} from "./loadProjectMedia";
import { normalizeLoadedSegment } from "./normalizeLoadedSegment";
import { recoverLegacyProjectSubtitleCopies } from "@/lib/legacySubtitleRecovery";

// ============================================================================
// useProjects
// ============================================================================
export type ProjectLoadResult = boolean | null | (() => boolean);

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
  onProjectLoaded?: (project: Project) => void | false | (() => boolean);
  onProjectLoadStart?: (projectId: string) => (applied: boolean) => void;
  currentVideo: string | null;
  currentAudio: string | null;
  currentMicAudio: string | null;
  currentWebcamVideo?: string | null;
}

function revokePreviousBlobUrl(
  previousUrl: string | null | undefined,
  nextUrl?: string,
) {
  if (previousUrl?.startsWith("blob:") && previousUrl !== nextUrl) {
    URL.revokeObjectURL(previousUrl);
  }
}

type ProjectListEntry = Omit<Project, "videoBlob" | "audioBlob" | "micAudioBlob" | "webcamBlob">;

export function useProjects(props: UseProjectsProps) {
  const [projects, setProjects] = useState<ProjectListEntry[]>([]);
  const projectsRef = useRef<ProjectListEntry[]>([]);
  const publishProjects = useCallback((next: ProjectListEntry[]) => {
    projectsRef.current = next;
    setProjects(next);
  }, []);
  const [showProjectsDialog, setShowProjectsDialog] = useState(false);
  const [currentProjectId, setCurrentProjectIdState] = useState<string | null>(null);
  const setCurrentProjectId = useCallback((projectId: string | null) => {
    projectManager.setActiveProjectId(projectId);
    setCurrentProjectIdState(projectId);
  }, []);
  const loadRequestSeqRef = useRef(0);
  const finishLoadRef = useRef<((applied: boolean) => void) | null>(null);
  const [, setLoadRevision] = useState(0);
  const cancelPendingProjectLoad = useCallback(() => {
    ++loadRequestSeqRef.current;
    finishLoadRef.current?.(false);
    finishLoadRef.current = null;
    setLoadRevision((revision) => revision + 1);
  }, []);
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
    publishProjects(await projectManager.getProjects());
    reportStartupMilestone("projects-hydrated");
  }, [publishProjects]);

  // After one project is saved, re-read only that record. The full list read
  // (every record with its segment and mouse data) is needed only when the
  // stored set changed underneath the list, e.g. retention pruned a project.
  const refreshProjectEntry = useCallback(async (projectId: string) => {
    const { project, count } = await projectManager.getProjectListEntry(projectId);
    const current = projectsRef.current;
    const listed = current.some((entry) => entry.id === projectId);
    if (!project || current.length + (listed ? 0 : 1) !== count) {
      await loadProjects();
      return;
    }
    // Replace in place so cards with equal sort keys keep their positions.
    const next = listed
      ? current.map((entry) => (entry.id === projectId ? project : entry))
      : [...current, project];
    publishProjects(sortProjectsByDisplayOrder(next));
  }, [loadProjects, publishProjects]);

  const handleLoadProject = useCallback(
    async (projectId: string) => {
      const loadRequestSeq = ++loadRequestSeqRef.current;
      const finishLoad = props.onProjectLoadStart?.(projectId);
      finishLoadRef.current = finishLoad ?? null;
      setLoadRevision((revision) => revision + 1);
      let applied = false;
      try {
      logProjectLoad("load:start", { projectId });
      const storedProject = await projectManager.loadProject(projectId);
      if (loadRequestSeq !== loadRequestSeqRef.current) return null;
      if (!storedProject) {
        logProjectLoad("load:missing", { projectId });
        return false;
      }
      let project = storedProject;
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
      let isTimelineOnlyProject =
        project.composition?.timelineOnly ||
        project.segment?.mediaMode === "timelineOnly" ||
        Boolean((project.composition as { srtWorkspace?: boolean } | undefined)?.srtWorkspace) ||
        Boolean(project.composition?.placeholderVideoForSubtitles && !project.rawVideoPath);

      const previousVideoUrl = props.currentVideo;
      const previousAudioUrl = props.currentAudio;
      const previousMicAudioUrl = props.currentMicAudio;
      const previousWebcamVideoUrl = props.currentWebcamVideo;

      // Restore rawVideoPath for old projects that only have a blob.
      // Writes the blob to disk via the media server POST endpoint (binary, no JSON overhead).
      let rawVideoPath = project.rawVideoPath ?? "";
      if (isTimelineOnlyProject) {
        try {
          const legacyDuration = Math.max(
            project.duration ?? 0,
            project.segment?.trimEnd ?? 0,
            ...getVisibleSubtitleSegments(project.segment)
              .map((subtitle) => subtitle.endTime)
              .filter(Number.isFinite),
            1,
          );
          const placeholder = await createAudioPlaceholderVideo(
            legacyDuration,
            "legacy-srt-placeholder",
          );
          rawVideoPath = placeholder.path;
          project.segment = {
            ...project.segment,
            mediaMode: undefined,
            trimStart: 0,
            trimEnd: legacyDuration,
            trimSegments: [
              {
                id: project.segment.trimSegments?.[0]?.id ?? crypto.randomUUID(),
                startTime: 0,
                endTime: legacyDuration,
              },
            ],
          };
          project.duration = legacyDuration;
          if (project.composition) {
            const composition = project.composition;
            project.composition = {
              ...composition,
              timelineOnly: false,
              placeholderVideoForSubtitles: true,
              clips: composition.clips.map((clip) =>
                clip.id === "root"
                  ? {
                      ...clip,
                      duration: legacyDuration,
                      segment: project.segment,
                      rawVideoPath,
                    }
                  : clip,
              ),
              globalSegment: composition.globalSegment
                ? project.segment
                : composition.globalSegment,
            };
            delete (project.composition as { srtWorkspace?: boolean }).srtWorkspace;
          }
          await projectManager.updateProject(projectId, {
            ...project,
            rawVideoPath,
          });
          isTimelineOnlyProject = false;
        } catch (e) {
          console.error("[ProjectLoad] Failed to materialize legacy timeline-only project:", e);
          rawVideoPath = "";
        }
      }
      if (!isTimelineOnlyProject && !rawVideoPath && project.videoBlob && project.videoBlob.size > 0) {
        try {
          if (project.recordingMode === "imported") {
            const restored = await importVideoToManagedMediaFile(
              project.videoBlob,
              `${project.name || "imported-video"}.mp4`,
            );
            rawVideoPath = restored.path;
            if (project.segment) {
              project.segment = {
                ...project.segment,
                deviceAudioAvailable: restored.hasAudio,
              };
            }
          } else {
            rawVideoPath = await writeBlobToTempMediaFile(project.videoBlob);
          }
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
      if (
        !isTimelineOnlyProject &&
        rawVideoPath &&
        project.recordingMode === "imported" &&
        !isManagedImportedVideoPath(rawVideoPath)
      ) {
        try {
          const response = await fetch(await getMediaServerUrl(rawVideoPath));
          if (response.ok) {
            const restored = await importVideoToManagedMediaFile(
              await response.blob(),
              `${project.name || "imported-video"}.mp4`,
            );
            rawVideoPath = restored.path;
            if (project.segment) {
              project.segment = {
                ...project.segment,
                deviceAudioAvailable: restored.hasAudio,
              };
            }
            await projectManager.updateProject(projectId, {
              ...project,
              rawVideoPath,
            });
          }
        } catch (e) {
          console.error("[ProjectLoad] Failed to normalize imported rawVideoPath:", e);
        }
      }
      let rawMicAudioPath = project.rawMicAudioPath ?? "";
      if (!rawMicAudioPath && project.micAudioBlob && project.micAudioBlob.size > 0) {
        rawMicAudioPath = await restoreRawPath(
          project.micAudioBlob,
          "rawMicAudioPath",
          projectId,
          project,
          (restoredPath) => ({ rawVideoPath, rawMicAudioPath: restoredPath }),
        );
      }
      let rawWebcamVideoPath = project.rawWebcamVideoPath ?? "";
      if (!rawWebcamVideoPath && project.webcamBlob && project.webcamBlob.size > 0) {
        rawWebcamVideoPath = await restoreRawPath(
          project.webcamBlob,
          "rawWebcamVideoPath",
          projectId,
          project,
          (restoredPath) => ({
            rawVideoPath,
            rawMicAudioPath,
            rawWebcamVideoPath: restoredPath,
          }),
        );
      }
      if (loadRequestSeq !== loadRequestSeqRef.current) return null;

      const videoObjectUrl = await loadProjectVideo({
        controller: props.videoControllerRef.current,
        project,
        isTimelineOnlyProject,
        rawVideoPath,
        isLatestRequest: () => loadRequestSeq === loadRequestSeqRef.current,
      });
      if (loadRequestSeq !== loadRequestSeqRef.current) return null;

      const { audioObjectUrl, micAudioObjectUrl, webcamVideoObjectUrl } =
        await loadProjectAudioMedia({
          controller: props.videoControllerRef.current,
          project,
          rawVideoPath,
          rawMicAudioPath,
          rawWebcamVideoPath,
          videoObjectUrl,
        });
      if (loadRequestSeq !== loadRequestSeqRef.current) return null;

      const videoDuration = isTimelineOnlyProject
        ? Math.max(project.duration ?? 0, project.segment.trimEnd, 1)
        : props.videoControllerRef.current?.duration || 0;
      // Resolve legacy clip-local caption copies after every load migration, so
      // the editor segment and the composition share one recovered state.
      project = recoverLegacyProjectSubtitleCopies(project);
      const correctedSegment = normalizeLoadedSegment({
        project,
        isTimelineOnlyProject,
        videoDuration,
        rawWebcamVideoPath,
        micAudioObjectUrl,
        webcamVideoObjectUrl,
      });

      // Draw the first frame on the canvas immediately (before React state updates)
      // so the canvas has content when the projects overlay fades out.
      if (!isTimelineOnlyProject) {
        props.videoControllerRef.current?.renderImmediate({
          segment: correctedSegment,
          backgroundConfig: cloneBackgroundConfig(project.backgroundConfig),
          webcamConfig: cloneWebcamConfig(project.webcamConfig),
          mousePositions: project.mousePositions,
        });
      }

      setCurrentProjectId(projectId);
      props.setThumbnails([]);
      if (videoObjectUrl) {
        props.setCurrentVideo(videoObjectUrl);
        revokePreviousBlobUrl(previousVideoUrl, videoObjectUrl);
      } else {
        props.setCurrentVideo(null);
        revokePreviousBlobUrl(previousVideoUrl);
      }
      if (audioObjectUrl) {
        props.setCurrentAudio(audioObjectUrl);
        revokePreviousBlobUrl(previousAudioUrl, audioObjectUrl);
      } else {
        props.setCurrentAudio(null);
        revokePreviousBlobUrl(previousAudioUrl);
      }
      if (micAudioObjectUrl) {
        props.setCurrentMicAudio(micAudioObjectUrl);
        revokePreviousBlobUrl(previousMicAudioUrl, micAudioObjectUrl);
      } else {
        props.setCurrentMicAudio(null);
        revokePreviousBlobUrl(previousMicAudioUrl);
      }
      if (webcamVideoObjectUrl) {
        props.setCurrentWebcamVideo(webcamVideoObjectUrl);
        revokePreviousBlobUrl(previousWebcamVideoUrl, webcamVideoObjectUrl);
      } else {
        props.setCurrentWebcamVideo(null);
        revokePreviousBlobUrl(previousWebcamVideoUrl);
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
      const owner = props.onProjectLoaded?.({
        ...project,
        rawVideoPath,
        rawMicAudioPath,
        rawWebcamVideoPath,
        segment: correctedSegment,
      });
      applied = true;
      logProjectLoad("load:applied", {
        projectId,
        canvasMode: project.backgroundConfig?.canvasMode,
        canvasWidth: project.backgroundConfig?.canvasWidth,
        canvasHeight: project.backgroundConfig?.canvasHeight,
      });
      // Ensure keyboard focus returns to the document after the Projects overlay
      // animates out (clone removal can leave focus in limbo → spacebar ignored).
      requestAnimationFrame(() => document.body.focus());
      return typeof owner === 'function' ? owner : owner !== false;
      } catch (error) {
        if (loadRequestSeq !== loadRequestSeqRef.current) return null;
        throw error;
      } finally {
        finishLoad?.(applied);
        if (loadRequestSeq === loadRequestSeqRef.current) {
          finishLoadRef.current = null;
          setLoadRevision((revision) => revision + 1);
        }
      }
    },
    [props],
  );

  useEffect(() => {
    loadProjects();
  }, [loadProjects]);

  useEffect(() => () => {
    ++loadRequestSeqRef.current;
    finishLoadRef.current?.(false);
    finishLoadRef.current = null;
    projectManager.setActiveProjectId(null);
  }, []);

  return {
    projects,
    showProjectsDialog,
    setShowProjectsDialog,
    currentProjectId,
    setCurrentProjectId,
    loadProjects,
    refreshProjectEntry,
    handleLoadProject,
    cancelPendingProjectLoad,
  };
}
