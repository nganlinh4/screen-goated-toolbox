import {
  useState,
  useEffect,
  useCallback,
  useRef,
  useMemo,
  type MutableRefObject,
} from "react";
import {
  BackgroundConfig,
  MousePosition,
  Project,
  ProjectComposition,
  ProjectCompositionMode,
  VideoSegment,
  RecordingMode,
} from "@/types/video";
import {
  applyCanvasConfig,
  extractCanvasConfig,
  getCompositionAdjacentClipIds,
  getCompositionClip,
  getEffectiveCompositionMode,
  syncCompositionCanvasConfig,
  updateCompositionClip,
  withCompositionSelection,
} from "@/lib/projectComposition";
import type { VideoController } from "@/lib/videoController";
import * as operations from "./operations";

export type PersistOptions = {
  refreshList?: boolean;
  includeMedia?: boolean;
  compositionOverride?: ProjectComposition;
  skipLiveCompositionSync?: boolean;
  allowDuringProjectTransition?: boolean;
};

export interface UseSequenceCompositionParams {
  // project context
  currentProjectId: string | null;
  // composition state (owned by App.tsx, managed here)
  composition: ProjectComposition | null;
  setComposition: (c: ProjectComposition | null | ((prev: ProjectComposition | null) => ProjectComposition | null)) => void;
  currentProjectData: Project | null;
  setCurrentProjectData: (p: Project | null) => void;
  // live editor state
  backgroundConfig: BackgroundConfig;
  segment: VideoSegment | null;
  mousePositions: MousePosition[];
  duration: number;
  currentRawVideoPath: string;
  currentRecordingMode: RecordingMode;
  loadedClipId: string | null;
  // switching guards
  isSwitchingCompositionClipRef: MutableRefObject<boolean>;
  isProjectTransitionRef: MutableRefObject<boolean>;
  // clip media cache refs (for eviction in handleRemoveSequenceClip)
  clipLoadRequestSeqRef: MutableRefObject<number>;
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
  clearClipMediaCaches: (options?: {
    preserveVideoUrl?: string | null;
    preserveAudioUrl?: string | null;
    preserveMicAudioUrl?: string | null;
    preserveWebcamVideoUrl?: string | null;
  }) => void;
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
  primePreloadSlot: (
    slot: "previous" | "next",
    clipId: string | null,
    projectOverride?: Project | null,
    compositionOverride?: ProjectComposition | null,
  ) => Promise<void>;
  // persistence — stable ref to avoid circular dependency
  persistRef: MutableRefObject<((opts?: PersistOptions) => Promise<void>) | null>;
  // playback
  seek: (time: number) => void;
  videoControllerRef: MutableRefObject<VideoController | undefined>;
  isPlaying: boolean;
  currentTime: number;
  togglePlayback: () => void;
  // projects dialog
  setShowProjectsDialog: (show: boolean) => void;
}

export function useSequenceComposition({
  currentProjectId,
  composition,
  setComposition,
  currentProjectData,
  setCurrentProjectData,
  backgroundConfig,
  segment,
  mousePositions,
  duration,
  currentRawVideoPath,
  currentRecordingMode,
  loadedClipId,
  isSwitchingCompositionClipRef,
  isProjectTransitionRef,
  clipLoadRequestSeqRef,
  loadClipMediaIntoEditor,
  clearClipMediaCaches,
  clipAssetCacheRef,
  clipUrlCacheRef,
  clipExportSourcePathCacheRef,
  clipExportMicAudioPathCacheRef,
  clipExportWebcamPathCacheRef,
  preloadedSlotClipIdsRef,
  primePreloadSlot,
  persistRef,
  seek,
  videoControllerRef,
  isPlaying,
  currentTime,
  togglePlayback,
  setShowProjectsDialog,
}: UseSequenceCompositionParams) {
  const [projectPickerMode, setProjectPickerMode] = useState<
    "insertBefore" | "insertAfter" | null
  >(null);
  const [sequenceTargetClipId, setSequenceTargetClipId] = useState<string | null>(null);
  const [spreadFromClipId, setSpreadFromClipId] = useState<string | null>(null);
  const spreadAnimationTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const hasSequenceChain = (composition?.clips.length ?? 0) > 1;
  const selectedClipId = hasSequenceChain
    ? (composition?.focusedClipId ?? composition?.selectedClipId ?? null)
    : null;
  const activeClipId = hasSequenceChain ? (loadedClipId ?? selectedClipId) : null;
  const compositionSyncClipId = composition
    ? hasSequenceChain
      ? activeClipId
      : "root"
    : null;
  const activeCompositionClip = useMemo(
    () => (hasSequenceChain ? getCompositionClip(composition, activeClipId) : null),
    [activeClipId, composition, hasSequenceChain],
  );
  const { previousClipId, nextClipId } = useMemo(
    () =>
      hasSequenceChain
        ? getCompositionAdjacentClipIds(composition, activeClipId)
        : { previousClipId: null, nextClipId: null },
    [activeClipId, composition, hasSequenceChain],
  );

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      if (spreadAnimationTimerRef.current) {
        clearTimeout(spreadAnimationTimerRef.current);
      }
      clearClipMediaCaches();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Preload adjacent clips whenever adjacency changes
  useEffect(() => {
    if (!hasSequenceChain || !currentProjectId || !currentProjectData || !composition)
      return;
    void primePreloadSlot("previous", previousClipId, currentProjectData, composition);
    void primePreloadSlot("next", nextClipId, currentProjectData, composition);
  }, [
    composition,
    currentProjectData,
    hasSequenceChain,
    nextClipId,
    previousClipId,
    primePreloadSlot,
    currentProjectId,
  ]);

  // Composition sync effect: keeps the active clip's metadata in sync with live
  // editor state (segment, backgroundConfig, mousePositions, etc.)
  useEffect(() => {
    if (
      !composition ||
      !segment ||
      !compositionSyncClipId ||
      isSwitchingCompositionClipRef.current ||
      isProjectTransitionRef.current
    )
      return;
    setComposition((prev) => {
      if (!prev) return prev;
      const canvasConfig = extractCanvasConfig(backgroundConfig);
      let next = syncCompositionCanvasConfig(prev, canvasConfig);
      const effectiveMode = getEffectiveCompositionMode(prev);
      if (next.mode !== effectiveMode) {
        next = {
          ...next,
          mode: effectiveMode,
        };
      }
      const currentClipBackground =
        getCompositionClip(next, compositionSyncClipId)?.backgroundConfig ??
        applyCanvasConfig(backgroundConfig, canvasConfig);
      next = updateCompositionClip(next, compositionSyncClipId, {
        segment,
        backgroundConfig:
          effectiveMode === "separate"
            ? applyCanvasConfig(backgroundConfig, canvasConfig)
            : currentClipBackground,
        mousePositions,
        duration: Math.max(duration, segment.trimEnd),
        recordingMode: currentRecordingMode,
        rawVideoPath: currentRawVideoPath || undefined,
      });
      if (effectiveMode === "unified") {
        next = {
          ...next,
          unifiedSourceClipId:
            prev.unifiedSourceClipId ?? compositionSyncClipId,
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
      return next;
    });
  }, [
    backgroundConfig,
    composition?.focusedClipId,
    composition?.selectedClipId,
    composition?.mode,
    compositionSyncClipId,
    currentRawVideoPath,
    currentRecordingMode,
    duration,
    mousePositions,
    segment,
  ]); // eslint-disable-line react-hooks/exhaustive-deps

  // Auto-advance to next clip when the current clip reaches its end during playback
  useEffect(() => {
    if (!hasSequenceChain || !activeCompositionClip || !nextClipId || !isPlaying)
      return;
    const activeEndTime = activeCompositionClip.segment.trimEnd;
    const remaining = activeEndTime - currentTime;
    if (remaining > 0.04) return;
    if (isSwitchingCompositionClipRef.current) return;
    const upcomingClip = getCompositionClip(composition, nextClipId);
    if (!upcomingClip) return;
    void focusCompositionClip(nextClipId, {
      seekTime: upcomingClip.segment.trimStart,
      playAfterLoad: true,
    });
  }, [
    activeCompositionClip,
    composition,
    currentTime,
    hasSequenceChain,
    isPlaying,
    nextClipId,
  ]); // eslint-disable-line react-hooks/exhaustive-deps

  const focusCompositionClip = useCallback(
    async (
      clipId: string,
      options?: { seekTime?: number; playAfterLoad?: boolean },
    ) => {
      if (!currentProjectId || !composition) return;
      const requestId = clipLoadRequestSeqRef.current + 1;
      const nextComposition = withCompositionSelection(composition, clipId);
      const targetClip = getCompositionClip(nextComposition, clipId);
      if (!targetClip) return;
      setComposition(nextComposition);
      await loadClipMediaIntoEditor(
        currentProjectId,
        clipId,
        currentProjectData,
        nextComposition,
        {
          preferPreloadedFrame: true,
          requestId,
        },
      );
      if (clipLoadRequestSeqRef.current !== requestId) return;
      const targetSeekTime =
        typeof options?.seekTime === "number"
          ? options.seekTime
          : targetClip.segment.trimStart;
      await new Promise<void>((resolve) =>
        requestAnimationFrame(() => resolve()),
      );
      if (clipLoadRequestSeqRef.current !== requestId) return;
      seek(targetSeekTime);
      if (options?.playAfterLoad) {
        videoControllerRef.current?.play();
      }
    },
    [
      composition,
      currentProjectData,
      loadClipMediaIntoEditor,
      currentProjectId,
      seek,
      videoControllerRef,
    ],
  );

  const handleTogglePlayPause = useCallback(() => {
    if (isSwitchingCompositionClipRef.current) {
      return;
    }

    if (
      hasSequenceChain &&
      !isPlaying &&
      composition &&
      activeCompositionClip &&
      currentTime >= activeCompositionClip.segment.trimEnd - 0.04
    ) {
      const targetClipId = nextClipId ?? composition.clips[0]?.id ?? null;
      if (targetClipId && targetClipId !== activeClipId) {
        const targetClip = getCompositionClip(composition, targetClipId);
        void focusCompositionClip(targetClipId, {
          seekTime: targetClip?.segment.trimStart,
          playAfterLoad: true,
        });
        return;
      }
      if (targetClipId && activeCompositionClip) {
        seek(activeCompositionClip.segment.trimStart);
        requestAnimationFrame(() => {
          videoControllerRef.current?.play();
        });
        return;
      }
    }

    togglePlayback();
  }, [
    activeClipId,
    activeCompositionClip,
    composition,
    currentTime,
    focusCompositionClip,
    hasSequenceChain,
    isPlaying,
    nextClipId,
    togglePlayback,
  ]); // eslint-disable-line react-hooks/exhaustive-deps

  const handleOpenInsertProjectPicker = useCallback(
    (clipId: string | null, placement: "before" | "after") => {
      setSequenceTargetClipId(clipId);
      setProjectPickerMode(
        placement === "before" ? "insertBefore" : "insertAfter",
      );
      setShowProjectsDialog(true);
    },
    [setShowProjectsDialog],
  );

  const handlePickProjectForSequence = useCallback(
    async (projectId: string) => {
      if (!currentProjectId || !composition) return;
      await operations.pickProjectForSequence(projectId, {
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
      });
    },
    [
      backgroundConfig,
      composition,
      currentProjectData,
      currentProjectId,
      loadClipMediaIntoEditor,
      persistRef,
      projectPickerMode,
      sequenceTargetClipId,
      setShowProjectsDialog,
    ],
  );

  const handleSelectSequenceClip = useCallback(
    async (clipId: string) => {
      const targetClip = getCompositionClip(composition, clipId);
      if (!targetClip) return;
      if (clipId === loadedClipId && !isSwitchingCompositionClipRef.current) {
        seek(targetClip.segment.trimStart);
        if (isPlaying) {
          videoControllerRef.current?.play();
        }
        return;
      }
      await focusCompositionClip(clipId, {
        seekTime: targetClip.segment.trimStart,
        playAfterLoad: isPlaying,
      });
    },
    [
      composition,
      focusCompositionClip,
      isPlaying,
      isSwitchingCompositionClipRef,
      loadedClipId,
      seek,
      videoControllerRef,
    ],
  );

  const handleRemoveSequenceClip = useCallback(
    async (clipId: string) => {
      if (!currentProjectId || !composition) return;
      await operations.removeSequenceClip(clipId, {
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
      });
    },
    [
      clipAssetCacheRef,
      clipExportMicAudioPathCacheRef,
      clipExportSourcePathCacheRef,
      clipExportWebcamPathCacheRef,
      clipUrlCacheRef,
      composition,
      currentProjectData,
      currentProjectId,
      loadClipMediaIntoEditor,
      persistRef,
      preloadedSlotClipIdsRef,
    ],
  );

  const handleSequenceModeChange = useCallback(
    async (mode: ProjectCompositionMode) => {
      if (!composition || !currentProjectId) return;
      await operations.changeSequenceMode(mode, {
        currentProjectId,
        composition,
        backgroundConfig,
        currentProjectData,
        spreadAnimationTimerRef,
        setComposition,
        setSpreadFromClipId,
        loadClipMediaIntoEditor,
        persistRef,
      });
    },
    [
      backgroundConfig,
      composition,
      currentProjectData,
      currentProjectId,
      loadClipMediaIntoEditor,
      persistRef,
    ],
  );

  return {
    composition,
    setComposition,
    currentProjectData,
    setCurrentProjectData,
    projectPickerMode,
    setProjectPickerMode,
    sequenceTargetClipId,
    setSequenceTargetClipId,
    setSpreadFromClipId,
    spreadFromClipId,
    spreadAnimationTimerRef,
    hasSequenceChain,
    selectedClipId,
    activeClipId,
    compositionSyncClipId,
    activeCompositionClip,
    previousClipId,
    nextClipId,
    focusCompositionClip,
    handleTogglePlayPause,
    handleOpenInsertProjectPicker,
    handlePickProjectForSequence,
    handleSelectSequenceClip,
    handleRemoveSequenceClip,
    handleSequenceModeChange,
  };
}
