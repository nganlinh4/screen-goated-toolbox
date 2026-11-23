import { closeNarrationProject } from './narrationProjectOwnership';
import { assertAudioImportWriteAllowed } from '@/lib/audioImportWriteFence';
import {
  useCallback,
  type Dispatch,
  type MutableRefObject,
  type SetStateAction,
} from "react";
import type {
  BackgroundConfig,
  MousePosition,
  Project,
  ProjectComposition,
  RecordingMode,
  VideoSegment,
  WebcamConfig,
} from "@/types/video";
import type { useEditorHistory } from "@/hooks/useEditorHistory";
import { projectManager } from "@/lib/projectManager";
import type { PersistOptions } from "@/hooks/useSequenceComposition";
import { useSettings } from "@/hooks/useSettings";

interface CloseProjectOptions {
  backgroundConfig: BackgroundConfig;
  currentAudio: string | null;
  currentMicAudio: string | null;
  currentRawMicAudioPath: string;
  currentRawVideoPath: string;
  currentRawWebcamVideoPath: string;
  currentRecordingMode: RecordingMode;
  currentProjectDataRef: MutableRefObject<Project | null>;
  currentProjectId: string | null;
  currentProjectIdRef: MutableRefObject<string | null>;
  currentVideo: string | null;
  currentWebcamVideo: string | null;
  editorHistory: ReturnType<typeof useEditorHistory>;
  historyProjectResetRef: MutableRefObject<string | null>;
  isProcessing: boolean;
  isRecording: boolean;
  beginProjectInteractionShield: () => void;
  endProjectInteractionShield: () => void;
  persistRef: MutableRefObject<
    ((options?: PersistOptions) => Promise<void>) | null
  >;
  projects: {
    setCurrentProjectId: (projectId: string | null) => void;
    cancelPendingProjectLoad?: () => void;
  };
  rawSetComposition: Dispatch<SetStateAction<ProjectComposition | null>>;
  rawSetSegment: Dispatch<SetStateAction<VideoSegment | null>>;
  setCurrentAudio: Dispatch<SetStateAction<string | null>>;
  setCurrentMicAudio: Dispatch<SetStateAction<string | null>>;
  setCurrentProjectData: Dispatch<SetStateAction<Project | null>>;
  setCurrentTime: Dispatch<SetStateAction<number>>;
  setCurrentVideo: Dispatch<SetStateAction<string | null>>;
  setCurrentWebcamVideo: Dispatch<SetStateAction<string | null>>;
  setLoadedClipId: (clipId: string | null) => void;
  setMousePositions: Dispatch<SetStateAction<MousePosition[]>>;
  setPreviewDuration: Dispatch<SetStateAction<number>>;
  setThumbnails: Dispatch<SetStateAction<string[]>>;
  setError: (message: string) => void;
  webcamConfig: WebcamConfig;
}

export function useCloseProject({
  backgroundConfig,
  currentAudio,
  currentMicAudio,
  currentRawMicAudioPath,
  currentRawVideoPath,
  currentRawWebcamVideoPath,
  currentRecordingMode,
  currentProjectDataRef,
  currentProjectId,
  currentProjectIdRef,
  currentVideo,
  currentWebcamVideo,
  editorHistory,
  historyProjectResetRef,
  isProcessing,
  isRecording,
  beginProjectInteractionShield,
  endProjectInteractionShield,
  persistRef,
  projects,
  rawSetComposition,
  rawSetSegment,
  setCurrentAudio,
  setCurrentMicAudio,
  setCurrentProjectData,
  setCurrentTime,
  setCurrentVideo,
  setCurrentWebcamVideo,
  setLoadedClipId,
  setMousePositions,
  setPreviewDuration,
  setThumbnails,
  setError,
  webcamConfig,
}: CloseProjectOptions) {
  const { t } = useSettings();
  return useCallback(async () => {
    if (isRecording || isProcessing) return false;
    try {
      assertAudioImportWriteAllowed(currentProjectIdRef.current ?? currentProjectId ?? '', true);
    } catch (error) {
      setError(error instanceof Error ? error.message : String(error));
      return false;
    }
    projects.cancelPendingProjectLoad?.();
    beginProjectInteractionShield();
    try {
      if (currentProjectId) {
        await persistRef.current?.({
          allowDuringProjectTransition: true,
          includeMedia: false,
          refreshList: false,
          throwOnError: true,
        });
        projectManager.invalidateEditorWrites(currentProjectId);
      }
    } catch (error) {
      console.error("Could not save current project before closing", error);
      setError(t.projectSaveFailed);
      endProjectInteractionShield();
      return false;
    }
    try {
      assertAudioImportWriteAllowed(currentProjectIdRef.current ?? currentProjectId ?? '', true);
    } catch (error) {
      setError(error instanceof Error ? error.message : String(error));
      endProjectInteractionShield();
      return false;
    }
    // A new load may have started while the current project was being saved.
    projects.cancelPendingProjectLoad?.();
    [currentVideo, currentAudio, currentMicAudio, currentWebcamVideo].forEach((url) => {
      if (url?.startsWith("blob:")) URL.revokeObjectURL(url);
    });
    editorHistory.withoutHistory(() => {
      setCurrentVideo(null);
      setCurrentAudio(null);
      setCurrentMicAudio(null);
      setCurrentWebcamVideo(null);
      rawSetSegment(null);
      setThumbnails([]);
      setMousePositions([]);
      setCurrentTime(0);
      setPreviewDuration(0);
      setLoadedClipId(null);
      rawSetComposition(null);
      setCurrentProjectData(null);
    });
    currentProjectDataRef.current = null;
    closeNarrationProject(currentProjectIdRef);
    projects.setCurrentProjectId(null);
    historyProjectResetRef.current = null;
    editorHistory.resetHistory({
      backgroundConfig,
      composition: null,
      currentRawMicAudioPath,
      currentRawVideoPath,
      currentRawWebcamVideoPath,
      currentRecordingMode,
      duration: 0,
      segment: null,
      webcamConfig,
    });
    endProjectInteractionShield();
    return true;
  }, [
    backgroundConfig,
    beginProjectInteractionShield,
    currentAudio,
    currentMicAudio,
    currentRawMicAudioPath,
    currentRawVideoPath,
    currentRawWebcamVideoPath,
    currentRecordingMode,
    currentProjectDataRef,
    currentProjectId,
    currentProjectIdRef,
    currentVideo,
    currentWebcamVideo,
    editorHistory,
    endProjectInteractionShield,
    historyProjectResetRef,
    isProcessing,
    isRecording,
    projects,
    persistRef,
    rawSetComposition,
    rawSetSegment,
    setCurrentAudio,
    setCurrentMicAudio,
    setCurrentProjectData,
    setCurrentTime,
    setCurrentVideo,
    setCurrentWebcamVideo,
    setLoadedClipId,
    setMousePositions,
    setPreviewDuration,
    setThumbnails,
    setError,
    t.projectSaveFailed,
    webcamConfig,
  ]);
}
