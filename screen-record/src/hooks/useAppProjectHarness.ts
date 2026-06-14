import { useEffect, useRef } from "react";
import { cloneBackgroundConfig } from "@/lib/backgroundConfig";
import { cloneWebcamConfig, DEFAULT_WEBCAM_CONFIG } from "@/lib/webcam";
import { installScreenRecordAppTestHarness } from "@/testHarness/appHarness";
import type {
  BackgroundConfig,
  Dispatch,
  EditorHistory,
  MousePosition,
  MutableRefObject,
  Project,
  ProjectComposition,
  ProjectsState,
  RecordingMode,
  SetStateAction,
  VideoSegment,
  WebcamConfig,
} from "@/hooks/appControllerTypes";

export interface AppProjectHarnessArgs {
  composition: ProjectComposition | null;
  currentProjectData: Project | null;
  currentProjectDataRef: MutableRefObject<Project | null>;
  currentProjectIdRef: MutableRefObject<string | null>;
  currentRawMicAudioPath: string;
  currentRawVideoPath: string;
  currentRawWebcamVideoPath: string;
  currentRecordingMode: RecordingMode;
  duration: number;
  editorHistory: EditorHistory;
  handleProjectRawVideoPathChange: (path: string) => void;
  projects: ProjectsState;
  rawSetComposition: Dispatch<SetStateAction<ProjectComposition | null>>;
  rawSetCurrentRawMicAudioPath: Dispatch<SetStateAction<string>>;
  rawSetCurrentRawWebcamVideoPath: Dispatch<SetStateAction<string>>;
  rawSetSegment: Dispatch<SetStateAction<VideoSegment | null>>;
  rawSetWebcamConfig: Dispatch<SetStateAction<WebcamConfig>>;
  segment: VideoSegment | null;
  segmentRef: MutableRefObject<VideoSegment | null>;
  setBackgroundConfigState: Dispatch<SetStateAction<BackgroundConfig>>;
  setCurrentAudio: (url: string | null) => void;
  setCurrentMicAudio: (url: string | null) => void;
  setCurrentProjectData: Dispatch<SetStateAction<Project | null>>;
  setCurrentTime: (time: number) => void;
  setCurrentVideo: (url: string | null) => void;
  setCurrentWebcamVideo: (url: string | null) => void;
  setMousePositions: (positions: MousePosition[]) => void;
  setPreviewDuration: (duration: number) => void;
  setThumbnails: (thumbnails: string[]) => void;
}

export function useAppProjectHarness(args: AppProjectHarnessArgs) {
  const {
    composition,
    currentProjectData,
    currentProjectDataRef,
    currentProjectIdRef,
    currentRawMicAudioPath,
    currentRawVideoPath,
    currentRawWebcamVideoPath,
    currentRecordingMode,
    duration,
    editorHistory,
    handleProjectRawVideoPathChange,
    projects,
    rawSetComposition,
    rawSetCurrentRawMicAudioPath,
    rawSetCurrentRawWebcamVideoPath,
    rawSetSegment,
    rawSetWebcamConfig,
    segment,
    segmentRef,
    setBackgroundConfigState,
    setCurrentAudio,
    setCurrentMicAudio,
    setCurrentProjectData,
    setCurrentTime,
    setCurrentVideo,
    setCurrentWebcamVideo,
    setMousePositions,
    setPreviewDuration,
    setThumbnails,
  } = args;
  const historyProjectResetRef = useRef<string | null>(null);

  useEffect(() => {
    currentProjectIdRef.current = projects.currentProjectId;
  }, [currentProjectIdRef, projects.currentProjectId]);

  useEffect(() => {
    return installScreenRecordAppTestHarness({
      loadProject: (project) => {
        editorHistory.withoutHistory(() => {
          currentProjectIdRef.current = project.id;
          currentProjectDataRef.current = project;
          setCurrentProjectData(project);
          rawSetSegment(project.segment);
          rawSetComposition(project.composition ?? null);
          setBackgroundConfigState(cloneBackgroundConfig(project.backgroundConfig));
          rawSetWebcamConfig(cloneWebcamConfig(project.webcamConfig ?? DEFAULT_WEBCAM_CONFIG));
          setPreviewDuration(project.duration ?? project.segment.trimEnd);
          setCurrentTime(0);
          handleProjectRawVideoPathChange(project.rawVideoPath ?? "");
          rawSetCurrentRawMicAudioPath(project.rawMicAudioPath ?? "");
          rawSetCurrentRawWebcamVideoPath(project.rawWebcamVideoPath ?? "");
          setCurrentVideo(null);
          setCurrentAudio(null);
          setCurrentMicAudio(null);
          setCurrentWebcamVideo(null);
          setThumbnails([]);
          setMousePositions(project.mousePositions ?? []);
        });
        projects.setCurrentProjectId(project.id);
        editorHistory.resetHistory({
          segment: project.segment,
          composition: project.composition ?? null,
          backgroundConfig: project.backgroundConfig,
          webcamConfig: project.webcamConfig ?? DEFAULT_WEBCAM_CONFIG,
          duration: project.duration ?? project.segment.trimEnd,
          currentRecordingMode,
          currentRawVideoPath: project.rawVideoPath ?? "",
          currentRawMicAudioPath: project.rawMicAudioPath ?? "",
          currentRawWebcamVideoPath: project.rawWebcamVideoPath ?? "",
        });
      },
      getProjectId: () => currentProjectIdRef.current,
      getDuration: () => duration,
      getSegment: () => currentProjectDataRef.current?.segment ?? segmentRef.current ?? segment,
      getComposition: () => currentProjectDataRef.current?.composition ?? composition,
      setCurrentTime,
    });
  }, [
    composition,
    currentProjectDataRef,
    currentProjectIdRef,
    currentRecordingMode,
    currentRawMicAudioPath,
    currentRawVideoPath,
    currentRawWebcamVideoPath,
    duration,
    editorHistory,
    handleProjectRawVideoPathChange,
    projects,
    rawSetComposition,
    rawSetCurrentRawMicAudioPath,
    rawSetCurrentRawWebcamVideoPath,
    rawSetSegment,
    rawSetWebcamConfig,
    segment,
    segmentRef,
    setBackgroundConfigState,
    setCurrentAudio,
    setCurrentMicAudio,
    setCurrentProjectData,
    setCurrentTime,
    setCurrentVideo,
    setCurrentWebcamVideo,
    setMousePositions,
    setPreviewDuration,
    setThumbnails,
  ]);

  useEffect(() => {
    const projectId = currentProjectData?.id ?? null;
    if (!projectId || historyProjectResetRef.current === projectId) return;
    historyProjectResetRef.current = projectId;
    editorHistory.resetHistory(editorHistory.getSnapshot());
  }, [currentProjectData?.id, editorHistory]);

  return historyProjectResetRef;
}
