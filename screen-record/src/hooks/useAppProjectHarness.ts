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
  startExport: () => Promise<unknown>;
}

export function useAppProjectHarness(args: AppProjectHarnessArgs) {
  const historyProjectResetRef = useRef<string | null>(null);
  const latestArgsRef = useRef(args);
  latestArgsRef.current = args;

  useEffect(() => {
    args.currentProjectIdRef.current = args.projects.currentProjectId;
  }, [args.currentProjectIdRef, args.projects.currentProjectId]);

  useEffect(() => {
    return installScreenRecordAppTestHarness({
      loadProject: (project) => {
        const current = latestArgsRef.current;
        current.editorHistory.withoutHistory(() => {
          current.currentProjectIdRef.current = project.id;
          current.currentProjectDataRef.current = project;
          current.setCurrentProjectData(project);
          current.rawSetSegment(project.segment);
          current.rawSetComposition(project.composition ?? null);
          current.setBackgroundConfigState(cloneBackgroundConfig(project.backgroundConfig));
          current.rawSetWebcamConfig(cloneWebcamConfig(project.webcamConfig ?? DEFAULT_WEBCAM_CONFIG));
          current.setPreviewDuration(project.duration ?? project.segment.trimEnd);
          current.setCurrentTime(0);
          current.handleProjectRawVideoPathChange(project.rawVideoPath ?? "");
          current.rawSetCurrentRawMicAudioPath(project.rawMicAudioPath ?? "");
          current.rawSetCurrentRawWebcamVideoPath(project.rawWebcamVideoPath ?? "");
          current.setCurrentVideo(null);
          current.setCurrentAudio(null);
          current.setCurrentMicAudio(null);
          current.setCurrentWebcamVideo(null);
          current.setThumbnails([]);
          current.setMousePositions(project.mousePositions ?? []);
        });
        current.projects.setCurrentProjectId(project.id);
        current.editorHistory.resetHistory({
          segment: project.segment,
          composition: project.composition ?? null,
          backgroundConfig: project.backgroundConfig,
          webcamConfig: project.webcamConfig ?? DEFAULT_WEBCAM_CONFIG,
          duration: project.duration ?? project.segment.trimEnd,
          currentRecordingMode: current.currentRecordingMode,
          currentRawVideoPath: project.rawVideoPath ?? "",
          currentRawMicAudioPath: project.rawMicAudioPath ?? "",
          currentRawWebcamVideoPath: project.rawWebcamVideoPath ?? "",
        });
      },
      getProjectId: () => latestArgsRef.current.currentProjectIdRef.current,
      getDuration: () => latestArgsRef.current.duration,
      getSegment: () => {
        const current = latestArgsRef.current;
        return current.currentProjectDataRef.current?.segment ?? current.segmentRef.current ?? current.segment;
      },
      getComposition: () => {
        const current = latestArgsRef.current;
        return current.currentProjectDataRef.current?.composition ?? current.composition;
      },
      setCurrentVideoSource: (source) => latestArgsRef.current.setCurrentVideo(source),
      setCurrentTime: (time) => latestArgsRef.current.setCurrentTime(time),
      startExport: () => latestArgsRef.current.startExport(),
    });
  }, []);

  useEffect(() => {
    const projectId = args.currentProjectData?.id ?? null;
    if (!projectId || historyProjectResetRef.current === projectId) return;
    historyProjectResetRef.current = projectId;
    args.editorHistory.resetHistory(args.editorHistory.getSnapshot());
  }, [args.currentProjectData?.id, args.editorHistory]);

  return historyProjectResetRef;
}
