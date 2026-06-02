import { useAppDropActions, usePendingAutoSubtitleGeneration } from "@/hooks/useAppDropActions";
import { useAppMediaImports } from "@/hooks/useAppMediaImports";
import { useTimelineTrackCallbacks } from "@/hooks/useTimelineTrackCallbacks";
import { useTimelineWorkspaceComposition } from "@/hooks/useTimelineWorkspaceComposition";

type AppTimelineControllerArgs = Record<string, any>;

export function useAppTimelineControllers(args: AppTimelineControllerArgs) {
  const {
    composition,
    currentProjectDataRef,
    currentProjectId,
    currentProjectIdRef,
    currentRawVideoPath,
    currentTime,
    duration,
    editorHistory,
    handleEditorRawVideoPathChange,
    handleGenerateSubtitles,
    isGeneratingSubtitles,
    isPlaceholderBackedProject,
    isPlaying,
    loadProjects,
    pendingAutoSubtitleArmed,
    pendingAutoSubtitleProjectId,
    projects,
    rawSetComposition,
    rawSetSegment,
    segment,
    segmentRef,
    selectedSubtitleIdsRef,
    selectedTextIdsRef,
    setActivePanel,
    setComposition,
    setCompositionSilently,
    setCurrentProjectData,
    setCurrentVideo,
    setEditorPreviewDuration,
    setEditingSubtitleId,
    setPendingAutoSubtitleArmed,
    setPendingAutoSubtitleProjectId,
    setSegment,
    setSubtitleSource,
    subtitleSource,
    videoControllerRef,
    videoRef,
  } = args;

  const timelineWorkspace = useTimelineWorkspaceComposition({
    composition,
    currentProjectDataRef,
    currentProjectId,
    currentProjectIdRef,
    currentTime,
    duration,
    editorHistory,
    handleEditorRawVideoPathChange,
    isPlaceholderBackedProject,
    isPlaying,
    loadProjects,
    rawSetComposition,
    rawSetSegment,
    segmentRef,
    setComposition,
    setCompositionSilently,
    setCurrentProjectData,
    setCurrentVideo,
    setEditorPreviewDuration,
    setSegment,
    videoControllerRef,
    videoRef,
  });

  const trackCallbacks = useTimelineTrackCallbacks({
    applyCurrentComposition: timelineWorkspace.applyCurrentComposition,
    composition,
    currentProjectDataRef,
    duration,
    isPlaceholderBackedProject,
    persistCurrentComposition: timelineWorkspace.persistCurrentComposition,
    segmentRef,
    setComposition,
    setCurrentProjectData,
    setEditorPreviewDuration,
    setSegment,
    updateCurrentMusicSegments: timelineWorkspace.updateCurrentMusicSegments,
    updateCurrentNarrationSegments: timelineWorkspace.updateCurrentNarrationSegments,
    updatePlaceholderProjectDuration: timelineWorkspace.updatePlaceholderProjectDuration,
  });

  const mediaImports = useAppMediaImports({
    composition,
    currentProjectDataRef,
    currentProjectIdRef,
    currentTime,
    duration,
    isPlaceholderBackedProject,
    persistTimelineWorkspaceState: timelineWorkspace.persistTimelineWorkspaceState,
    projects,
    segment,
    segmentRef,
    selectedSubtitleIdsRef,
    selectedTextIdsRef,
    setActivePanel,
    setComposition,
    setCurrentVideo,
    setSegment,
    setEditingSubtitleId,
    setSubtitleSource,
    updateCurrentMusicSegments: timelineWorkspace.updateCurrentMusicSegments,
    videoControllerRef,
  });

  useAppDropActions({
    importAudioPaths: mediaImports.importAudioPaths,
    importSubtitlePayload: mediaImports.importSubtitlePayload,
    importVideoPath: mediaImports.importVideoPath,
    setPendingAutoSubtitleProjectId,
  });

  usePendingAutoSubtitleGeneration({
    currentProjectId,
    currentRawVideoPath,
    handleGenerateSubtitles,
    isGeneratingSubtitles,
    pendingAutoSubtitleArmed,
    pendingAutoSubtitleProjectId,
    setActivePanel,
    setPendingAutoSubtitleArmed,
    setPendingAutoSubtitleProjectId,
    setSubtitleSource,
    subtitleSource,
  });

  return {
    ...timelineWorkspace,
    ...trackCallbacks,
    ...mediaImports,
  };
}
