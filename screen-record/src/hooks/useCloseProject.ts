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

interface CloseProjectOptions {
  backgroundConfig: BackgroundConfig;
  currentAudio: string | null;
  currentMicAudio: string | null;
  currentRawMicAudioPath: string;
  currentRawVideoPath: string;
  currentRawWebcamVideoPath: string;
  currentRecordingMode: RecordingMode;
  currentVideo: string | null;
  currentWebcamVideo: string | null;
  editorHistory: ReturnType<typeof useEditorHistory>;
  historyProjectResetRef: MutableRefObject<string | null>;
  isProcessing: boolean;
  isRecording: boolean;
  projects: {
    setCurrentProjectId: (projectId: string | null) => void;
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
  currentVideo,
  currentWebcamVideo,
  editorHistory,
  historyProjectResetRef,
  isProcessing,
  isRecording,
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
  webcamConfig,
}: CloseProjectOptions) {
  return useCallback(() => {
    if (isRecording || isProcessing) return;
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
  }, [
    backgroundConfig,
    currentAudio,
    currentMicAudio,
    currentRawMicAudioPath,
    currentRawVideoPath,
    currentRawWebcamVideoPath,
    currentRecordingMode,
    currentVideo,
    currentWebcamVideo,
    editorHistory,
    historyProjectResetRef,
    isProcessing,
    isRecording,
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
    webcamConfig,
  ]);
}
