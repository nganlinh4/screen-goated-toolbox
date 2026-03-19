import { useCallback, type MutableRefObject, type RefObject } from "react";
import {
  BackgroundConfig,
  VideoSegment,
  ProjectComposition,
  WebcamConfig,
  Project,
} from "@/types/video";
import { cloneBackgroundConfig } from "@/lib/backgroundConfig";
import { cloneWebcamConfig } from "@/lib/webcam";
import {
  ensureProjectComposition,
  getCompositionResolvedBackgroundConfig,
} from "@/lib/projectComposition";
import {
  summarizeBackgroundConfig,
  summarizeSegment,
  toPreviewRectSnapshot,
} from "@/lib/appUtils";

export interface UseProjectActionsParams {
  // From useProjects
  projects: {
    currentProjectId: string | null;
    showProjectsDialog: boolean;
    setShowProjectsDialog: (show: boolean) => void;
    handleLoadProject: (id: string) => Promise<void>;
  };
  // From useProjectInteractionShield
  isProjectInteractionShieldVisible: boolean;
  isProjectTransitionRef: MutableRefObject<boolean>;
  projectInteractionShieldReleaseRef: MutableRefObject<(() => void) | null>;
  projectInteractionBlockCleanupRef: MutableRefObject<(() => void) | null>;
  beginProjectInteractionShield: () => void;
  abortEditorInteractions: () => void;
  setIsProjectInteractionShieldVisible: (v: boolean) => void;
  armProjectInteractionShieldRelease: () => void;
  // From useClipMediaCache
  clearClipMediaCaches: (opts: {
    preserveVideoUrl: string | null;
    preserveAudioUrl: string | null;
    preserveMicAudioUrl: string | null;
    preserveWebcamVideoUrl: string | null;
  }) => void;
  clipExportSourcePathCacheRef: MutableRefObject<Map<string, string>>;
  clipExportWebcamPathCacheRef: MutableRefObject<Map<string, string | null>>;
  isSwitchingCompositionClipRef: MutableRefObject<boolean>;
  clipLoadRequestSeqRef: MutableRefObject<number>;
  loadClipMediaIntoEditor: (
    projectId: string,
    clipId: string,
    project: Project,
    composition: ProjectComposition,
  ) => Promise<void>;
  setLoadedClipId: (id: string | null) => void;
  // App state setters
  currentVideo: string | null;
  currentAudio: string | null;
  currentMicAudio: string | null;
  currentWebcamVideo: string | null;
  backgroundConfig: BackgroundConfig;
  segment: VideoSegment | null;
  currentProjectData: Project | null;
  setCurrentProjectData: (p: Project | null) => void;
  setCurrentRawMicAudioPath: (path: string) => void;
  setCurrentRawWebcamVideoPath: (path: string) => void;
  setWebcamConfig: (config: WebcamConfig) => void;
  setComposition: (c: ProjectComposition | null) => void;
  applyLoadedBackgroundConfig: (config: BackgroundConfig) => void;
  spreadAnimationTimerRef: MutableRefObject<ReturnType<typeof setTimeout> | null>;
  setSpreadFromClipId: (id: string | null) => void;
  setLastCaptureFps: (fps: number | null) => void;
  // Refs for preview snapshots
  canvasRef: RefObject<HTMLCanvasElement | null>;
  previewContainerRef: MutableRefObject<HTMLDivElement | null>;
  restoreImageRef: MutableRefObject<string | null>;
  projectsPreviewTargetSnapshotRef: MutableRefObject<{
    stageRect: ReturnType<typeof toPreviewRectSnapshot>;
    canvasRect: ReturnType<typeof toPreviewRectSnapshot>;
  } | null>;
  // Debug helpers from useProjectPersistence
  debugProject: (event: string, data?: Record<string, unknown>) => void;
  logProjectSwitch: (event: string, data?: Record<string, unknown>) => void;
  // Persist callback
  persistCurrentProjectNow: (opts?: {
    refreshList?: boolean;
    includeMedia?: boolean;
  }) => Promise<void>;
}

export interface UseProjectActionsResult {
  onProjectLoaded: (project: Project) => void;
  handleLoadProjectFromGrid: (projectId: string) => Promise<void>;
  requestCloseProjects: () => void;
  handleToggleProjects: () => Promise<void>;
}

export function useProjectActions({
  projects,
  isProjectInteractionShieldVisible,
  isProjectTransitionRef,
  projectInteractionShieldReleaseRef,
  projectInteractionBlockCleanupRef,
  beginProjectInteractionShield,
  abortEditorInteractions,
  setIsProjectInteractionShieldVisible,
  clearClipMediaCaches,
  clipExportSourcePathCacheRef,
  clipExportWebcamPathCacheRef,
  isSwitchingCompositionClipRef,
  clipLoadRequestSeqRef,
  loadClipMediaIntoEditor,
  setLoadedClipId,
  currentVideo,
  currentAudio,
  currentMicAudio,
  currentWebcamVideo,
  backgroundConfig,
  segment,
  currentProjectData,
  setCurrentProjectData,
  setCurrentRawMicAudioPath,
  setCurrentRawWebcamVideoPath,
  setWebcamConfig,
  setComposition,
  applyLoadedBackgroundConfig,
  spreadAnimationTimerRef,
  setSpreadFromClipId,
  setLastCaptureFps,
  canvasRef,
  previewContainerRef,
  restoreImageRef,
  projectsPreviewTargetSnapshotRef,
  debugProject,
  logProjectSwitch,
  persistCurrentProjectNow,
}: UseProjectActionsParams): UseProjectActionsResult {
  const onProjectLoaded = useCallback(
    (project: Project) => {
      clearClipMediaCaches({
        preserveVideoUrl: currentVideo,
        preserveAudioUrl: currentAudio,
        preserveMicAudioUrl: currentMicAudio,
        preserveWebcamVideoUrl: currentWebcamVideo,
      });
      clipExportSourcePathCacheRef.current.clear();
      clipExportWebcamPathCacheRef.current.clear();
      isSwitchingCompositionClipRef.current = true;
      clipLoadRequestSeqRef.current += 1;
      setCurrentProjectData({
        ...project,
        backgroundConfig: cloneBackgroundConfig(project.backgroundConfig),
        webcamConfig: cloneWebcamConfig(project.webcamConfig),
      });
      setCurrentRawMicAudioPath(project.rawMicAudioPath ?? "");
      setCurrentRawWebcamVideoPath(project.rawWebcamVideoPath ?? "");
      setWebcamConfig(cloneWebcamConfig(project.webcamConfig));
      const nextComposition = ensureProjectComposition(project);
      setComposition(nextComposition);
      if (spreadAnimationTimerRef.current) {
        clearTimeout(spreadAnimationTimerRef.current);
      }
      setSpreadFromClipId(null);
      const nextClipId =
        nextComposition.focusedClipId ?? nextComposition.selectedClipId;
      if (nextClipId === "root") {
        const resolvedRootBackground =
          getCompositionResolvedBackgroundConfig(nextComposition, "root") ??
          project.backgroundConfig;
        applyLoadedBackgroundConfig(
          cloneBackgroundConfig(resolvedRootBackground),
        );
        setLoadedClipId("root");
        requestAnimationFrame(() => {
          requestAnimationFrame(() => {
            isSwitchingCompositionClipRef.current = false;
          });
        });
        return;
      }
      setLoadedClipId(null);
      if (nextClipId) {
        void loadClipMediaIntoEditor(
          project.id,
          nextClipId,
          project,
          nextComposition,
        ).finally(() => {
          requestAnimationFrame(() => {
            requestAnimationFrame(() => {
              isSwitchingCompositionClipRef.current = false;
            });
          });
        });
      } else {
        requestAnimationFrame(() => {
          isSwitchingCompositionClipRef.current = false;
        });
      }
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [
      clearClipMediaCaches,
      currentVideo,
      currentAudio,
      currentMicAudio,
      currentWebcamVideo,
    ],
  );

  const requestCloseProjects = useCallback(() => {
    if (!projects.showProjectsDialog) return;
    window.dispatchEvent(new CustomEvent("sr-close-projects"));
  }, [projects.showProjectsDialog]);

  const handleLoadProjectFromGrid = useCallback(
    async (projectId: string) => {
      debugProject("grid-load:start", {
        targetProjectId: projectId,
        currentProjectId: projects.currentProjectId,
      });
      logProjectSwitch("grid-load:start", {
        targetProjectId: projectId,
        currentProjectId: projects.currentProjectId,
        currentProjectDataId: currentProjectData?.id ?? null,
        currentBackground: summarizeBackgroundConfig(backgroundConfig),
        currentSegment: summarizeSegment(segment),
      });
      if (projectId === projects.currentProjectId) {
        projectInteractionShieldReleaseRef.current?.();
        isProjectTransitionRef.current = false;
        setIsProjectInteractionShieldVisible(false);
        projectInteractionBlockCleanupRef.current?.();
        projects.setShowProjectsDialog(false);
        debugProject("grid-load:same-project-close", {
          targetProjectId: projectId,
        });
        return;
      }
      beginProjectInteractionShield();
      abortEditorInteractions();
      projectInteractionShieldReleaseRef.current?.();
      await persistCurrentProjectNow({
        refreshList: false,
        includeMedia: false,
      });
      setLastCaptureFps(null);
      try {
        await projects.handleLoadProject(projectId);
      } catch (err) {
        isProjectTransitionRef.current = false;
        setIsProjectInteractionShieldVisible(false);
        throw err;
      }
      debugProject("grid-load:done", { targetProjectId: projectId });
    },
    [
      abortEditorInteractions,
      backgroundConfig,
      beginProjectInteractionShield,
      currentProjectData?.id,
      debugProject,
      isProjectTransitionRef,
      logProjectSwitch,
      persistCurrentProjectNow,
      projectInteractionBlockCleanupRef,
      projectInteractionShieldReleaseRef,
      projects,
      segment,
      setIsProjectInteractionShieldVisible,
      setLastCaptureFps,
    ],
  );

  const handleToggleProjects = useCallback(async () => {
    if (isProjectInteractionShieldVisible) return;
    if (projects.showProjectsDialog) {
      debugProject("projects-toggle:close");
      requestCloseProjects();
      return;
    }
    debugProject("projects-toggle:open:start", {
      currentProjectId: projects.currentProjectId,
      canvasMode: backgroundConfig.canvasMode,
      canvasWidth: backgroundConfig.canvasWidth,
      canvasHeight: backgroundConfig.canvasHeight,
    });
    void persistCurrentProjectNow({ refreshList: true, includeMedia: false });
    if (canvasRef.current && currentVideo) {
      try {
        restoreImageRef.current = canvasRef.current.toDataURL("image/jpeg", 0.8);
      } catch {
        restoreImageRef.current = null;
      }
    } else {
      restoreImageRef.current = null;
    }
    projectsPreviewTargetSnapshotRef.current = {
      stageRect: toPreviewRectSnapshot(
        previewContainerRef.current?.getBoundingClientRect() ?? null,
      ),
      canvasRect: toPreviewRectSnapshot(
        canvasRef.current?.getBoundingClientRect() ?? null,
      ),
    };
    projects.setShowProjectsDialog(true);
    debugProject("projects-toggle:open:done", {
      currentProjectId: projects.currentProjectId,
    });
  }, [
    backgroundConfig.canvasHeight,
    backgroundConfig.canvasMode,
    backgroundConfig.canvasWidth,
    canvasRef,
    currentVideo,
    debugProject,
    isProjectInteractionShieldVisible,
    persistCurrentProjectNow,
    previewContainerRef,
    projectsPreviewTargetSnapshotRef,
    projects,
    requestCloseProjects,
    restoreImageRef,
  ]);

  return {
    onProjectLoaded,
    handleLoadProjectFromGrid,
    requestCloseProjects,
    handleToggleProjects,
  };
}
