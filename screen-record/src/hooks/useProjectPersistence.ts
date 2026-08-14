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
import { normalizeSubtitleTrackState } from "@/lib/subtitleTracks";
import type { PersistOptions } from "@/hooks/useSequenceComposition";
import type { ClipMediaAssets } from "@/hooks/useClipMediaCache";

function preserveProjectLevelAudioLanes(
  nextComposition: ProjectComposition,
  fallbackComposition: ProjectComposition | null | undefined,
): ProjectComposition {
  if (!fallbackComposition) return nextComposition;
  const shouldPreserveAudio =
    nextComposition.audioSegments === undefined &&
    fallbackComposition.audioSegments !== undefined;
  const shouldPreserveNarration =
    nextComposition.narrationSegments === undefined &&
    fallbackComposition.narrationSegments !== undefined;
  const shouldPreserveAudioVolume =
    nextComposition.audioTrackVolumePoints === undefined &&
    fallbackComposition.audioTrackVolumePoints !== undefined;
  const shouldPreserveNarrationVolume =
    nextComposition.narrationTrackVolumePoints === undefined &&
    fallbackComposition.narrationTrackVolumePoints !== undefined;
  const activeClipIds = new Set(nextComposition.clips.map((clip) => clip.id));
  const retainedRemovedClips = (
    nextComposition.retainedRemovedClips ??
    fallbackComposition.retainedRemovedClips
  )?.filter((clip) => !activeClipIds.has(clip.id));
  if (
    !shouldPreserveAudio &&
    !shouldPreserveNarration &&
    !shouldPreserveAudioVolume &&
    !shouldPreserveNarrationVolume &&
    retainedRemovedClips === nextComposition.retainedRemovedClips
  ) {
    return nextComposition;
  }
  return {
    ...nextComposition,
    audioSegments: shouldPreserveAudio
      ? fallbackComposition.audioSegments
      : nextComposition.audioSegments,
    narrationSegments: shouldPreserveNarration
      ? fallbackComposition.narrationSegments
      : nextComposition.narrationSegments,
    audioTrackVolumePoints: shouldPreserveAudioVolume
      ? fallbackComposition.audioTrackVolumePoints
      : nextComposition.audioTrackVolumePoints,
    narrationTrackVolumePoints: shouldPreserveNarrationVolume
      ? fallbackComposition.narrationTrackVolumePoints
      : nextComposition.narrationTrackVolumePoints,
    retainedRemovedClips,
  };
}

function normalizeCompositionSubtitleState(
  composition: ProjectComposition,
): ProjectComposition {
  return {
    ...composition,
    clips: composition.clips.map((clip) => ({
      ...clip,
      segment: normalizeSubtitleTrackState(clip.segment),
    })),
    retainedRemovedClips: composition.retainedRemovedClips?.map((clip) => ({
      ...clip,
      segment: normalizeSubtitleTrackState(clip.segment),
    })),
    globalSegment: composition.globalSegment
      ? normalizeSubtitleTrackState(composition.globalSegment)
      : composition.globalSegment,
  };
}

export interface UseProjectPersistenceParams {
  currentProjectId: string | null;
  currentProjectIdRef: MutableRefObject<string | null>;
  projects: { projects: Project[]; loadProjects: () => Promise<void> };
  currentVideo: string | null;
  currentAudio: string | null;
  currentMicAudio: string | null;
  currentWebcamVideo: string | null;
  loadedClipId: string | null;
  currentProjectData: Project | null;
  currentProjectDataRef: MutableRefObject<Project | null>;
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
  currentProjectIdRef,
  projects,
  currentVideo,
  currentAudio,
  currentMicAudio,
  currentWebcamVideo,
  loadedClipId,
  currentProjectData,
  currentProjectDataRef,
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
  const persistenceInputsRef = useRef<readonly unknown[]>([]);
  const persistenceTokenRef = useRef<object>({});
  const persistenceInputs = [
    currentProjectId,
    currentProjectData,
    loadedClipId,
    segment,
    composition,
    backgroundConfig,
    mousePositions,
    duration,
    currentRecordingMode,
    currentRawVideoPath,
    currentRawMicAudioPath,
    currentRawWebcamVideoPath,
    webcamConfig,
  ] as const;
  if (
    persistenceInputs.length !== persistenceInputsRef.current.length ||
    persistenceInputs.some(
      (value, index) => value !== persistenceInputsRef.current[index],
    )
  ) {
    persistenceInputsRef.current = persistenceInputs;
    persistenceTokenRef.current = {};
  }

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
      const liveProject = currentProjectDataRef.current;
      const compositionState =
        options?.compositionOverride ?? liveProject?.composition ?? composition;
      const shouldSyncLiveComposition = !options?.skipLiveCompositionSync;
      const projectId = liveProject?.id ?? currentProjectData?.id ?? null;
      if (
        !projectId ||
        (currentProjectIdRef.current ?? currentProjectId) !== projectId ||
        !compositionState ||
        (!options?.allowDuringProjectTransition &&
          isProjectTransitionRef.current) ||
        (shouldSyncLiveComposition && isSwitchingCompositionClipRef.current) ||
        (shouldSyncLiveComposition && !segment)
      ) {
        return;
      }
      const saveSeq = ++projectSaveSeqRef.current;
      const writeIntent = projectManager.createEditorWriteIntent(projectId);
      const persistenceToken = persistenceTokenRef.current;
      const isCurrentSave = () =>
        saveSeq === projectSaveSeqRef.current &&
        projectManager.isEditorWriteIntentLatest(writeIntent) &&
        currentProjectIdRef.current === projectId &&
        currentProjectDataRef.current?.id === projectId &&
        persistenceTokenRef.current === persistenceToken;
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
        if (!isCurrentSave()) return;
        let videoBlob: Blob | undefined;
        let micAudioBlob: Blob | undefined;
        let webcamBlob: Blob | undefined;
        let thumbnail: string | undefined;
        if (activeClip.role === "root" && !options?.skipThumbnail) {
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
            if (!isCurrentSave()) return;
          }
          micAudioBlob =
            (loadedAssets?.micAudioBlob ?? currentProjectData?.micAudioBlob) ?? undefined;
          if (!micAudioBlob && currentMicAudio && !activeClip.rawMicAudioPath) {
            const response = await fetch(currentMicAudio);
            micAudioBlob = await response.blob();
            if (!isCurrentSave()) return;
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
            if (!isCurrentSave()) return;
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
            if (!isCurrentSave()) return;
          }
          if (!snapshotVideoBlob) return;
          let snapshotAudioBlob = loadedAssets?.audioBlob ?? undefined;
          if (!snapshotAudioBlob && currentAudio) {
            const audioResponse = await fetch(currentAudio);
            snapshotAudioBlob = await audioResponse.blob();
            if (!isCurrentSave()) return;
          }
          let snapshotMicAudioBlob = loadedAssets?.micAudioBlob ?? undefined;
          if (!snapshotMicAudioBlob && currentMicAudio && !activeClip.rawMicAudioPath) {
            const micAudioResponse = await fetch(currentMicAudio);
            snapshotMicAudioBlob = await micAudioResponse.blob();
            if (!isCurrentSave()) return;
          }
          let snapshotWebcamBlob = loadedAssets?.webcamBlob ?? undefined;
          if (
            !snapshotWebcamBlob &&
            currentWebcamVideo &&
            !activeClip.rawWebcamVideoPath
          ) {
            const webcamResponse = await fetch(currentWebcamVideo);
            snapshotWebcamBlob = await webcamResponse.blob();
            if (!isCurrentSave()) return;
          }
          const assetWriteApplied = await projectManager.saveCompositionClipAssets(
            projectId,
            activeClip.id,
            {
              videoBlob: snapshotVideoBlob,
              audioBlob: snapshotAudioBlob,
              micAudioBlob: snapshotMicAudioBlob,
              webcamBlob: snapshotWebcamBlob,
              customBackground: backgroundConfig.customBackground,
            },
            writeIntent,
          );
          if (!assetWriteApplied || !isCurrentSave()) return;
        }
        // Drop stale in-flight saves so older state never overwrites newer edits.
        if (!isCurrentSave()) {
          debugProject("persist:stale-before-write", {
            saveSeq,
            latestSeq: projectSaveSeqRef.current,
            projectId,
          });
          return;
        }
        const rootClip = getCompositionClip(nextComposition, "root");
        if (!rootClip) return;
        const storedProject = await projectManager.loadProject(projectId);
        if (!isCurrentSave()) return;
        nextComposition = preserveProjectLevelAudioLanes(
          nextComposition,
          storedProject?.composition ?? currentProjectData?.composition,
        );
        nextComposition = normalizeCompositionSubtitleState(nextComposition);
        const nextRootClip = getCompositionClip(nextComposition, "root");
        if (!nextRootClip) return;
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
        const projectUpdates: Partial<
          Omit<Project, "id" | "createdAt" | "lastModified">
        > = {
          segment: nextRootClip.segment,
          backgroundConfig: nextRootClip.backgroundConfig,
          webcamConfig:
            getCompositionClip(nextComposition, "root")?.webcamConfig ??
            cloneWebcamConfig(webcamConfig),
          mousePositions: nextRootClip.mousePositions,
          thumbnail:
            activeClip.role === "root"
              ? thumbnail
              : currentProjectData?.thumbnail,
          duration: nextRootClip.duration,
          recordingMode: nextRootClip.recordingMode ?? currentRecordingMode,
          rawVideoPath: nextRootClip.rawVideoPath,
          rawMicAudioPath: nextRootClip.rawMicAudioPath,
          rawWebcamVideoPath: nextRootClip.rawWebcamVideoPath,
          composition: nextComposition,
        };
        if (includeMedia) {
          projectUpdates.videoBlob = videoBlob;
          projectUpdates.micAudioBlob = micAudioBlob;
          projectUpdates.webcamBlob = webcamBlob;
        }
        const applied = await projectManager.updateProject(
          projectId,
          projectUpdates,
          writeIntent,
        );
        if (!applied || !isCurrentSave()) {
          debugProject("persist:stale-after-write", {
            saveSeq,
            latestSeq: projectSaveSeqRef.current,
            projectId,
          });
          return;
        }
        setComposition(nextComposition);
        debugProject("persist:committed", {
          saveSeq,
          projectId,
          canvasMode: backgroundConfig.canvasMode,
          canvasWidth: backgroundConfig.canvasWidth,
          canvasHeight: backgroundConfig.canvasHeight,
        });
        if (options?.refreshList !== false) {
          await projects.loadProjects();
          if (!isCurrentSave()) return;
          debugProject("persist:projects-refreshed", { saveSeq, projectId });
        }
      } catch (error) {
        debugProject("persist:error", {
          saveSeq,
          projectId,
          error: String(error),
        });
        if (options?.throwOnError) throw error;
        console.error("[ProjectSave] Failed to persist project", error);
      }
    },
    [
      currentProjectId,
      currentProjectIdRef,
      projects,
      currentVideo,
      currentAudio,
      currentMicAudio,
      currentWebcamVideo,
      loadedClipId,
      currentProjectData,
      currentProjectDataRef,
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
