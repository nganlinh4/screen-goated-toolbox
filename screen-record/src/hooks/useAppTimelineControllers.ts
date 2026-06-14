import { useAppDropActions, usePendingAutoSubtitleGeneration } from "@/hooks/useAppDropActions";
import { useAppMediaImports } from "@/hooks/useAppMediaImports";
import { useTimelineTrackCallbacks } from "@/hooks/useTimelineTrackCallbacks";
import { useTimelineWorkspaceComposition } from "@/hooks/useTimelineWorkspaceComposition";
import { type ActivePanel } from "@/components/sidepanel/index";
import type { SubtitleSource } from "@/lib/subtitleGenerationPlan";
import type { TrackSelectionRange } from "@/lib/timelineSegmentSelection";
import type { VideoController } from "@/lib/videoController";
import type {
  CompositionSetter,
  Dispatch,
  EditorHistory,
  MutableRefObject,
  Project,
  ProjectComposition,
  ProjectsState,
  RefObject,
  SegmentSetter,
  SetStateAction,
  VideoSegment,
} from "@/hooks/appControllerTypes";

export interface AppTimelineControllerArgs {
  composition: ProjectComposition | null;
  currentProjectDataRef: MutableRefObject<Project | null>;
  currentProjectId: string | null;
  currentProjectIdRef: MutableRefObject<string | null>;
  currentRawVideoPath: string;
  currentTime: number;
  duration: number;
  editorHistory: EditorHistory;
  handleEditorRawVideoPathChange: (value: string) => void;
  handleGenerateSubtitles: (selectedRange?: TrackSelectionRange | null) => void;
  isGeneratingSubtitles: boolean;
  isPlaceholderBackedProject: boolean;
  isPlaying: boolean;
  loadProjects: () => Promise<unknown>;
  pendingAutoSubtitleArmed: boolean;
  pendingAutoSubtitleProjectId: string | null;
  projects: ProjectsState;
  rawSetComposition: Dispatch<SetStateAction<ProjectComposition | null>>;
  rawSetSegment: Dispatch<SetStateAction<VideoSegment | null>>;
  segment: VideoSegment | null;
  segmentRef: MutableRefObject<VideoSegment | null>;
  selectedSubtitleIdsRef: MutableRefObject<string[]>;
  selectedTextIdsRef: MutableRefObject<string[]>;
  setActivePanel: (panel: ActivePanel) => void;
  setComposition: CompositionSetter;
  setCompositionSilently: CompositionSetter;
  setCurrentProjectData: Dispatch<SetStateAction<Project | null>>;
  setCurrentVideo: Dispatch<SetStateAction<string | null>>;
  setEditorPreviewDuration: (value: number) => void;
  setEditingSubtitleId: (id: string | null) => void;
  setPendingAutoSubtitleArmed: (armed: boolean) => void;
  setPendingAutoSubtitleProjectId: (id: string | null) => void;
  setSegment: SegmentSetter;
  setSubtitleSource: (source: SubtitleSource) => void;
  subtitleSource: SubtitleSource;
  videoControllerRef: MutableRefObject<VideoController | undefined>;
  videoRef: RefObject<HTMLVideoElement | null>;
}

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
