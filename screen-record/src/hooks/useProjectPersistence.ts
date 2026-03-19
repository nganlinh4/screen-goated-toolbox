import { useCallback, useRef, type MutableRefObject } from "react";
import {
  BackgroundConfig,
  MousePosition,
  Project,
  ProjectComposition,
  VideoSegment,
  RecordingMode,
  WebcamConfig,
} from "@/types/video";
import { projectManager } from "@/lib/projectManager";
import {
  applyCanvasConfig,
  extractCanvasConfig,
  getCompositionClip,
  getEffectiveCompositionMode,
  syncCompositionCanvasConfig,
  updateCompositionClip,
} from "@/lib/projectComposition";
import {
  PROJECT_SAVE_DEBUG,
  PROJECT_SWITCH_DEBUG,
  summarizeBackgroundConfig,
  summarizeSegment,
} from "@/lib/appUtils";
import { cloneWebcamConfig } from "@/lib/webcam";
import type { PersistOptions } from "@/hooks/useSequenceComposition";
import type { ClipMediaAssets } from "@/hooks/useClipMediaCache";

export interface UseProjectPersistenceParams {
  currentProjectId: string | null;
  projects: { projects: Project[]; loadProjects: () => Promise<void> };
  currentVideo: string | null;
  currentAudio: string | null;
  currentMicAudio: string | null;
  currentWebcamVideo: string | null;
  loadedClipId: string | null;
  currentProjectData: Project | null;
  segment: VideoSegment | null;
  composition: ProjectComposition | null;
  backgroundConfig: BackgroundConfig;
  mousePositions: MousePosition[];
  generateThumbnail: () => string | null | undefined;
  duration: number;
  currentRecordingMode: RecordingMode;
  currentRawVideoPath: string;
  currentRawMicAudioPath: string;
  currentRawWebcamVideoPath: string;
  webcamConfig: WebcamConfig;
  canvasRef: MutableRefObject<HTMLCanvasElement | null>;
  isProjectTransitionRef: MutableRefObject<boolean>;
  isSwitchingCompositionClipRef: MutableRefObject<boolean>;
  loadClipAssets: (
    projectId: string,
    clipId: string,
    projectData?: Project | null,
    composition?: ProjectComposition | null,
  ) => Promise<ClipMediaAssets | null>;
  setComposition: (c: ProjectComposition | null) => void;
}

export function useProjectPersistence({
  currentProjectId,
  projects,
  currentVideo,
  currentAudio,
  currentMicAudio,
  currentWebcamVideo,
  loadedClipId,
  currentProjectData,
  segment,
  composition,
  backgroundConfig,
  mousePositions,
  generateThumbnail,
  duration,
  currentRecordingMode,
  currentRawVideoPath,
  currentRawMicAudioPath,
  currentRawWebcamVideoPath,
  webcamConfig,
  canvasRef,
  isProjectTransitionRef,
  isSwitchingCompositionClipRef,
  loadClipAssets,
  setComposition,
}: UseProjectPersistenceParams) {
  const projectSaveSeqRef = useRef(0);

  const debugProject = useCallback(
    (event: string, data?: Record<string, unknown>) => {
      if (!PROJECT_SAVE_DEBUG) return;
      const ts = new Date().toISOString();
      console.log(`[ProjectSave][${ts}] ${event}`, data || {});
    },
    [],
  );

  const logProjectSwitch = useCallback(
    (event: string, data?: Record<string, unknown>) => {
      if (!PROJECT_SWITCH_DEBUG) return;
      console.warn(
        `[ProjectSwitch] ${JSON.stringify({
          event,
          ...data,
        })}`,
      );
    },
    [],
  );

  const persistCurrentProjectNow = useCallback(
    async (options?: PersistOptions) => {
      const compositionState = options?.compositionOverride ?? composition;
      const shouldSyncLiveComposition = !options?.skipLiveCompositionSync;
      const projectId = currentProjectData?.id ?? null;
      if (
        !projectId ||
        currentProjectId !== projectId ||
        !compositionState ||
        (!options?.allowDuringProjectTransition &&
          isProjectTransitionRef.current) ||
        (shouldSyncLiveComposition && isSwitchingCompositionClipRef.current) ||
        (shouldSyncLiveComposition && !segment)
      ) {
        return;
      }
      const saveSeq = ++projectSaveSeqRef.current;
      const includeMedia = options?.includeMedia !== false;
      const activeClipId = shouldSyncLiveComposition
        ? loadedClipId ??
          compositionState.focusedClipId ??
          compositionState.selectedClipId
        : compositionState.focusedClipId ?? compositionState.selectedClipId;
      const activeClip = activeClipId
        ? getCompositionClip(compositionState, activeClipId)
        : null;
      if (!activeClip) return;
      debugProject("persist:start", {
        saveSeq,
        projectId,
        refreshList: options?.refreshList ?? true,
        includeMedia,
        canvasMode: backgroundConfig.canvasMode,
        canvasWidth: backgroundConfig.canvasWidth,
        canvasHeight: backgroundConfig.canvasHeight,
      });
      try {
        const loadedAssets = await loadClipAssets(
          projectId,
          activeClip.id,
          currentProjectData,
          compositionState,
        );
        let videoBlob: Blob | undefined;
        let micAudioBlob: Blob | undefined;
        let webcamBlob: Blob | undefined;
        let thumbnail: string | undefined;
        if (activeClip.role === "root") {
          const canvasSnapshot = (() => {
            try {
              return canvasRef.current?.toDataURL("image/jpeg", 0.8);
            } catch {
              return undefined;
            }
          })();
          thumbnail =
            canvasSnapshot ||
            generateThumbnail() ||
            activeClip.thumbnail;
        }
        if (includeMedia && activeClip.role === "root") {
          videoBlob = (loadedAssets?.videoBlob ?? currentProjectData?.videoBlob) ?? undefined;
          if (!videoBlob && currentVideo && !currentRawVideoPath) {
            const response = await fetch(currentVideo);
            videoBlob = await response.blob();
          }
          micAudioBlob =
            (loadedAssets?.micAudioBlob ?? currentProjectData?.micAudioBlob) ?? undefined;
          if (!micAudioBlob && currentMicAudio && !activeClip.rawMicAudioPath) {
            const response = await fetch(currentMicAudio);
            micAudioBlob = await response.blob();
          }
          webcamBlob =
            (loadedAssets?.webcamBlob ?? currentProjectData?.webcamBlob) ?? undefined;
          if (
            !webcamBlob &&
            currentWebcamVideo &&
            !currentRawWebcamVideoPath
          ) {
            const response = await fetch(currentWebcamVideo);
            webcamBlob = await response.blob();
          }
        }
        const canvasConfig = extractCanvasConfig(backgroundConfig);
        let nextComposition = compositionState;
        if (shouldSyncLiveComposition) {
          nextComposition = syncCompositionCanvasConfig(
            nextComposition,
            canvasConfig,
          );
          const effectiveMode = getEffectiveCompositionMode(nextComposition);
          if (nextComposition.mode !== effectiveMode) {
            nextComposition = {
              ...nextComposition,
              mode: effectiveMode,
            };
          }
          nextComposition = updateCompositionClip(
            nextComposition,
            activeClip.id,
            {
              segment: segment!,
              backgroundConfig:
                effectiveMode === "separate"
                  ? applyCanvasConfig(backgroundConfig, canvasConfig)
                  : (getCompositionClip(nextComposition, activeClip.id)
                      ?.backgroundConfig ?? activeClip.backgroundConfig),
              mousePositions,
              duration: Math.max(duration, segment!.trimEnd),
              thumbnail:
                activeClip.role === "root"
                  ? (thumbnail ?? activeClip.thumbnail)
                  : activeClip.thumbnail,
              webcamConfig: cloneWebcamConfig(webcamConfig),
              recordingMode: currentRecordingMode,
              rawVideoPath: currentRawVideoPath || undefined,
              rawMicAudioPath: currentRawMicAudioPath || undefined,
              rawWebcamVideoPath: currentRawWebcamVideoPath || undefined,
            },
          );
          if (effectiveMode === "unified") {
            nextComposition = {
              ...nextComposition,
              globalPresentationConfig: applyCanvasConfig(
                backgroundConfig,
                canvasConfig,
              ),
              globalBackgroundConfig: applyCanvasConfig(
                backgroundConfig,
                canvasConfig,
              ),
            };
          }
        }
        if (
          includeMedia &&
          activeClip.role === "snapshot" &&
          (!currentRawVideoPath || !activeClip.rawMicAudioPath)
        ) {
          let snapshotVideoBlob = loadedAssets?.videoBlob ?? undefined;
          if (!snapshotVideoBlob && currentVideo) {
            const response = await fetch(currentVideo);
            snapshotVideoBlob = await response.blob();
          }
          if (!snapshotVideoBlob) return;
          let snapshotAudioBlob = loadedAssets?.audioBlob ?? undefined;
          if (!snapshotAudioBlob && currentAudio) {
            const audioResponse = await fetch(currentAudio);
            snapshotAudioBlob = await audioResponse.blob();
          }
          let snapshotMicAudioBlob = loadedAssets?.micAudioBlob ?? undefined;
          if (!snapshotMicAudioBlob && currentMicAudio && !activeClip.rawMicAudioPath) {
            const micAudioResponse = await fetch(currentMicAudio);
            snapshotMicAudioBlob = await micAudioResponse.blob();
          }
          let snapshotWebcamBlob = loadedAssets?.webcamBlob ?? undefined;
          if (
            !snapshotWebcamBlob &&
            currentWebcamVideo &&
            !activeClip.rawWebcamVideoPath
          ) {
            const webcamResponse = await fetch(currentWebcamVideo);
            snapshotWebcamBlob = await webcamResponse.blob();
          }
          await projectManager.saveCompositionClipAssets(
            projectId,
            activeClip.id,
            {
              videoBlob: snapshotVideoBlob,
              audioBlob: snapshotAudioBlob,
              micAudioBlob: snapshotMicAudioBlob,
              webcamBlob: snapshotWebcamBlob,
              customBackground: backgroundConfig.customBackground,
            },
          );
        }
        // Drop stale in-flight saves so older state never overwrites newer edits.
        if (saveSeq !== projectSaveSeqRef.current) {
          debugProject("persist:stale-before-write", {
            saveSeq,
            latestSeq: projectSaveSeqRef.current,
            projectId,
          });
          return;
        }
        const rootClip = getCompositionClip(nextComposition, "root");
        if (!rootClip) return;
        logProjectSwitch("persist:write-root", {
          targetProjectId: projectId,
          currentProjectDataId: currentProjectData?.id ?? null,
          saveSeq,
          activeClipId,
          rootBackground: summarizeBackgroundConfig(rootClip.backgroundConfig),
          rootSegment: summarizeSegment(rootClip.segment),
          editorBackground: summarizeBackgroundConfig(backgroundConfig),
          editorSegment: summarizeSegment(segment),
        });
        await projectManager.updateProject(projectId, {
          name:
            projects.projects.find((p) => p.id === projectId)?.name ||
            "Auto Saved",
          videoBlob,
          micAudioBlob,
          webcamBlob,
          segment: rootClip.segment,
          backgroundConfig: rootClip.backgroundConfig,
          webcamConfig:
            getCompositionClip(nextComposition, "root")?.webcamConfig ??
            cloneWebcamConfig(webcamConfig),
          mousePositions: rootClip.mousePositions,
          thumbnail:
            activeClip.role === "root"
              ? thumbnail
              : currentProjectData?.thumbnail,
          duration: rootClip.duration,
          recordingMode: rootClip.recordingMode ?? currentRecordingMode,
          rawVideoPath: rootClip.rawVideoPath,
          rawMicAudioPath: rootClip.rawMicAudioPath,
          rawWebcamVideoPath: rootClip.rawWebcamVideoPath,
          composition: nextComposition,
        });
        setComposition(nextComposition);
        if (saveSeq !== projectSaveSeqRef.current) {
          debugProject("persist:stale-after-write", {
            saveSeq,
            latestSeq: projectSaveSeqRef.current,
            projectId,
          });
          return;
        }
        debugProject("persist:committed", {
          saveSeq,
          projectId,
          canvasMode: backgroundConfig.canvasMode,
          canvasWidth: backgroundConfig.canvasWidth,
          canvasHeight: backgroundConfig.canvasHeight,
        });
        if (options?.refreshList !== false) {
          await projects.loadProjects();
          debugProject("persist:projects-refreshed", { saveSeq, projectId });
        }
      } catch (error) {
        debugProject("persist:error", {
          saveSeq,
          projectId,
          error: String(error),
        });
      }
    },
    [
      currentProjectId,
      projects,
      currentVideo,
      currentAudio,
      currentMicAudio,
      currentWebcamVideo,
      loadedClipId,
      currentProjectData,
      segment,
      composition,
      backgroundConfig,
      mousePositions,
      generateThumbnail,
      duration,
      debugProject,
      currentRecordingMode,
      currentRawVideoPath,
      currentRawMicAudioPath,
      currentRawWebcamVideoPath,
      loadClipAssets,
      webcamConfig,
      canvasRef,
      isProjectTransitionRef,
      isSwitchingCompositionClipRef,
      setComposition,
      logProjectSwitch,
    ],
  );

  return { persistCurrentProjectNow, debugProject, logProjectSwitch };
}
