import { invoke } from "@/lib/ipc";
import {
  BackgroundConfig,
  Project,
  ProjectComposition,
  ProjectCompositionMode,
} from "@/types/video";
import { projectManager } from "@/lib/projectManager";
import {
  applyCanvasConfig,
  createCompositionSnapshotClip,
  extractCanvasConfig,
  insertCompositionClip,
  normalizeCompositionClipToCanvas,
  removeCompositionClip,
  setCompositionMode,
  syncCompositionCanvasConfig,
} from "@/lib/projectComposition";
import { isManagedCompositionSnapshotPath } from "@/lib/mediaServer";
import type { PersistOptions } from "./index";
import type { MutableRefObject } from "react";

// ---------------------------------------------------------------------------
// pickProjectForSequence
// ---------------------------------------------------------------------------

export interface PickProjectForSequenceParams {
  currentProjectId: string;
  composition: ProjectComposition;
  backgroundConfig: BackgroundConfig;
  currentProjectData: Project | null;
  sequenceTargetClipId: string | null;
  projectPickerMode: "insertBefore" | "insertAfter" | null;
  setComposition: (
    c:
      | ProjectComposition
      | null
      | ((
          prev: ProjectComposition | null,
        ) => ProjectComposition | null),
  ) => void;
  setProjectPickerMode: (mode: "insertBefore" | "insertAfter" | null) => void;
  setShowProjectsDialog: (show: boolean) => void;
  loadClipMediaIntoEditor: (
    projectId: string,
    clipId: string,
    projectOverride?: Project | null,
    compositionOverride?: ProjectComposition | null,
    options?: {
      preferPreloadedFrame?: boolean;
      requestId?: number;
      deferThumbnailsMs?: number;
    },
  ) => Promise<void>;
  persistRef: MutableRefObject<((opts?: PersistOptions) => Promise<void>) | null>;
}

export async function pickProjectForSequence(
  projectId: string,
  params: PickProjectForSequenceParams,
): Promise<void> {
  const {
    currentProjectId,
    composition,
    backgroundConfig,
    currentProjectData,
    sequenceTargetClipId,
    projectPickerMode,
    setComposition,
    setProjectPickerMode,
    setShowProjectsDialog,
    loadClipMediaIntoEditor,
    persistRef,
  } = params;

  const pickedProject = await projectManager.loadProject(projectId);
  if (!pickedProject) return;
  let snapshotRawVideoPath: string | undefined;
  let snapshotRawMicAudioPath: string | undefined;
  let snapshotRawWebcamVideoPath: string | undefined;
  if (pickedProject.rawVideoPath) {
    try {
      const saved = await invoke<{ savedPath: string }>(
        "save_composition_snapshot_copy",
        {
          sourcePath: pickedProject.rawVideoPath,
        },
      );
      snapshotRawVideoPath = saved?.savedPath || undefined;
    } catch (error) {
      console.error(
        "[Composition] Failed to create native snapshot copy:",
        error,
      );
    }
  }
  if (pickedProject.rawMicAudioPath) {
    try {
      const saved = await invoke<{ savedPath: string }>(
        "save_composition_snapshot_copy",
        {
          sourcePath: pickedProject.rawMicAudioPath,
        },
      );
      snapshotRawMicAudioPath = saved?.savedPath || undefined;
    } catch (error) {
      console.error(
        "[Composition] Failed to create native snapshot mic copy:",
        error,
      );
    }
  }
  if (pickedProject.rawWebcamVideoPath) {
    try {
      const saved = await invoke<{ savedPath: string }>(
        "save_composition_snapshot_copy",
        {
          sourcePath: pickedProject.rawWebcamVideoPath,
        },
      );
      snapshotRawWebcamVideoPath = saved?.savedPath || undefined;
    } catch (error) {
      console.error(
        "[Composition] Failed to create native snapshot webcam copy:",
        error,
      );
    }
  }
  const snapshotClip = normalizeCompositionClipToCanvas(
    createCompositionSnapshotClip({
      ...pickedProject,
      rawVideoPath: snapshotRawVideoPath,
      rawMicAudioPath: snapshotRawMicAudioPath,
      rawWebcamVideoPath: snapshotRawWebcamVideoPath,
    }),
    composition.globalCanvasConfig ?? extractCanvasConfig(backgroundConfig),
  );
  if (!snapshotRawVideoPath && !pickedProject.videoBlob) {
    console.error(
      "[Composition] Insert failed: project has neither raw video path nor stored video blob",
    );
    return;
  }
  if (
    !snapshotRawVideoPath ||
    (!snapshotRawMicAudioPath && pickedProject.micAudioBlob) ||
    (!snapshotRawWebcamVideoPath && pickedProject.webcamBlob)
  ) {
    await projectManager.saveCompositionClipAssets(
      currentProjectId,
      snapshotClip.id,
      {
        videoBlob: !snapshotRawVideoPath ? pickedProject.videoBlob : undefined,
        audioBlob: !snapshotRawVideoPath ? pickedProject.audioBlob : undefined,
        micAudioBlob: !snapshotRawMicAudioPath
          ? pickedProject.micAudioBlob
          : undefined,
        webcamBlob: !snapshotRawWebcamVideoPath
          ? pickedProject.webcamBlob
          : undefined,
        customBackground: pickedProject.backgroundConfig.customBackground,
      },
    );
  }
  const nextComposition = insertCompositionClip(
    composition,
    sequenceTargetClipId,
    projectPickerMode === "insertBefore" ? "before" : "after",
    snapshotClip,
  );
  setComposition(nextComposition);
  setProjectPickerMode(null);
  setShowProjectsDialog(false);
  await loadClipMediaIntoEditor(
    currentProjectId,
    snapshotClip.id,
    currentProjectData,
    nextComposition,
  );
  void persistRef.current?.({
    refreshList: true,
    includeMedia: false,
    compositionOverride: nextComposition,
    skipLiveCompositionSync: true,
  });
}

// ---------------------------------------------------------------------------
// removeSequenceClip
// ---------------------------------------------------------------------------

export interface RemoveSequenceClipParams {
  currentProjectId: string;
  composition: ProjectComposition;
  currentProjectData: Project | null;
  clipAssetCacheRef: MutableRefObject<Map<string, unknown>>;
  clipUrlCacheRef: MutableRefObject<
    Map<
      string,
      {
        videoUrl: string;
        audioUrl: string | null;
        micAudioUrl: string | null;
        webcamVideoUrl: string | null;
      }
    >
  >;
  clipExportSourcePathCacheRef: MutableRefObject<Map<string, string>>;
  clipExportMicAudioPathCacheRef: MutableRefObject<Map<string, string | null>>;
  clipExportWebcamPathCacheRef: MutableRefObject<Map<string, string | null>>;
  preloadedSlotClipIdsRef: MutableRefObject<{
    previous: string | null;
    next: string | null;
  }>;
  setComposition: (
    c:
      | ProjectComposition
      | null
      | ((
          prev: ProjectComposition | null,
        ) => ProjectComposition | null),
  ) => void;
  loadClipMediaIntoEditor: (
    projectId: string,
    clipId: string,
    projectOverride?: Project | null,
    compositionOverride?: ProjectComposition | null,
    options?: {
      preferPreloadedFrame?: boolean;
      requestId?: number;
      deferThumbnailsMs?: number;
    },
  ) => Promise<void>;
  persistRef: MutableRefObject<((opts?: PersistOptions) => Promise<void>) | null>;
  getCompositionClip: (
    composition: ProjectComposition | null | undefined,
    clipId: string | null | undefined,
  ) => import("@/types/video").ProjectCompositionClip | null | undefined;
}

export async function removeSequenceClip(
  clipId: string,
  params: RemoveSequenceClipParams,
): Promise<void> {
  const {
    currentProjectId,
    composition,
    currentProjectData,
    clipAssetCacheRef,
    clipUrlCacheRef,
    clipExportSourcePathCacheRef,
    clipExportMicAudioPathCacheRef,
    clipExportWebcamPathCacheRef,
    preloadedSlotClipIdsRef,
    setComposition,
    loadClipMediaIntoEditor,
    persistRef,
    getCompositionClip,
  } = params;

  const clip = getCompositionClip(composition, clipId);
  if (!clip || clip.role === "root" || composition.clips.length <= 1) return;
  const nextComposition = removeCompositionClip(composition, clipId);
  setComposition(nextComposition);
  await projectManager.deleteCompositionClipAssets(currentProjectId, clipId);
  if (
    clip.rawVideoPath &&
    isManagedCompositionSnapshotPath(clip.rawVideoPath)
  ) {
    try {
      await invoke("delete_file", { path: clip.rawVideoPath });
    } catch {
      // ignore cleanup failures for snapshot media copies
    }
  }
  if (
    clip.rawMicAudioPath &&
    isManagedCompositionSnapshotPath(clip.rawMicAudioPath)
  ) {
    try {
      await invoke("delete_file", { path: clip.rawMicAudioPath });
    } catch {
      // ignore cleanup failures for snapshot media copies
    }
  }
  if (
    clip.rawWebcamVideoPath &&
    isManagedCompositionSnapshotPath(clip.rawWebcamVideoPath)
  ) {
    try {
      await invoke("delete_file", { path: clip.rawWebcamVideoPath });
    } catch {
      // ignore cleanup failures for snapshot media copies
    }
  }
  clipAssetCacheRef.current.delete(clipId);
  const cacheKey = `${currentProjectId}:${clipId}`;
  clipExportSourcePathCacheRef.current.delete(cacheKey);
  clipExportMicAudioPathCacheRef.current.delete(cacheKey);
  clipExportWebcamPathCacheRef.current.delete(cacheKey);
  const removedUrls = clipUrlCacheRef.current.get(clipId);
  if (removedUrls) {
    if (removedUrls.videoUrl.startsWith("blob:")) {
      URL.revokeObjectURL(removedUrls.videoUrl);
    }
    if (removedUrls.audioUrl?.startsWith("blob:")) {
      URL.revokeObjectURL(removedUrls.audioUrl);
    }
    if (removedUrls.micAudioUrl?.startsWith("blob:")) {
      URL.revokeObjectURL(removedUrls.micAudioUrl);
    }
    clipUrlCacheRef.current.delete(clipId);
  }
  if (preloadedSlotClipIdsRef.current.previous === clipId) {
    preloadedSlotClipIdsRef.current.previous = null;
  }
  if (preloadedSlotClipIdsRef.current.next === clipId) {
    preloadedSlotClipIdsRef.current.next = null;
  }
  const nextClipIdToLoad =
    nextComposition.focusedClipId ?? nextComposition.selectedClipId;
  if (nextClipIdToLoad) {
    await loadClipMediaIntoEditor(
      currentProjectId,
      nextClipIdToLoad,
      currentProjectData,
      nextComposition,
    );
  }
  void persistRef.current?.({
    refreshList: true,
    includeMedia: false,
    compositionOverride: nextComposition,
    skipLiveCompositionSync: true,
  });
}

// ---------------------------------------------------------------------------
// changeSequenceMode
// ---------------------------------------------------------------------------

export interface ChangeSequenceModeParams {
  currentProjectId: string;
  composition: ProjectComposition;
  backgroundConfig: BackgroundConfig;
  currentProjectData: Project | null;
  spreadAnimationTimerRef: MutableRefObject<ReturnType<typeof setTimeout> | null>;
  setComposition: (
    c:
      | ProjectComposition
      | null
      | ((
          prev: ProjectComposition | null,
        ) => ProjectComposition | null),
  ) => void;
  setSpreadFromClipId: (id: string | null) => void;
  loadClipMediaIntoEditor: (
    projectId: string,
    clipId: string,
    projectOverride?: Project | null,
    compositionOverride?: ProjectComposition | null,
    options?: {
      preferPreloadedFrame?: boolean;
      requestId?: number;
      deferThumbnailsMs?: number;
    },
  ) => Promise<void>;
  persistRef: MutableRefObject<((opts?: PersistOptions) => Promise<void>) | null>;
}

export async function changeSequenceMode(
  mode: ProjectCompositionMode,
  params: ChangeSequenceModeParams,
): Promise<void> {
  const {
    currentProjectId,
    composition,
    backgroundConfig,
    currentProjectData,
    spreadAnimationTimerRef,
    setComposition,
    setSpreadFromClipId,
    loadClipMediaIntoEditor,
    persistRef,
  } = params;

  const activeEditableClipId =
    composition.focusedClipId ?? composition.selectedClipId;
  if (!activeEditableClipId) return;
  const canvasConfig = extractCanvasConfig(backgroundConfig);
  let nextComposition = syncCompositionCanvasConfig(
    setCompositionMode(composition, mode),
    canvasConfig,
  );
  if (mode === "unified") {
    if (spreadAnimationTimerRef.current) {
      clearTimeout(spreadAnimationTimerRef.current);
    }
    setSpreadFromClipId(activeEditableClipId);
    spreadAnimationTimerRef.current = setTimeout(() => {
      setSpreadFromClipId(null);
    }, 900);
    nextComposition = {
      ...nextComposition,
      unifiedSourceClipId: activeEditableClipId,
      globalPresentationConfig: applyCanvasConfig(backgroundConfig, canvasConfig),
      globalBackgroundConfig: applyCanvasConfig(backgroundConfig, canvasConfig),
    };
  } else {
    if (spreadAnimationTimerRef.current) {
      clearTimeout(spreadAnimationTimerRef.current);
    }
    setSpreadFromClipId(null);
  }
  setComposition(nextComposition);
  const targetClipId =
    nextComposition.focusedClipId ?? nextComposition.selectedClipId;
  if (targetClipId) {
    await loadClipMediaIntoEditor(
      currentProjectId,
      targetClipId,
      currentProjectData,
      nextComposition,
    );
  }
  void persistRef.current?.({
    refreshList: false,
    includeMedia: false,
    compositionOverride: nextComposition,
    skipLiveCompositionSync: true,
  });
}
