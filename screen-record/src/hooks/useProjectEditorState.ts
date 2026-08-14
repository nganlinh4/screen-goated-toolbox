import {
  useCallback,
  useRef,
  useState,
  type Dispatch,
  type SetStateAction,
} from "react";
import type {
  Project,
  ProjectComposition,
  RecordingMode,
  VideoSegment,
  WebcamConfig,
} from "@/types/video";
import type { PersistOptions } from "@/hooks/useSequenceComposition";
import { cloneWebcamConfig, DEFAULT_WEBCAM_CONFIG } from "@/lib/webcam";

export function useProjectEditorState() {
  const [segment, rawSetSegment] = useState<VideoSegment | null>(null);
  const [webcamConfig, rawSetWebcamConfig] = useState<WebcamConfig>(() =>
    cloneWebcamConfig(DEFAULT_WEBCAM_CONFIG),
  );
  const [currentRecordingMode, rawSetCurrentRecordingMode] =
    useState<RecordingMode>("withoutCursor");
  const [currentProjectData, setCurrentProjectDataState] = useState<Project | null>(
    null,
  );
  const [composition, setCompositionState] =
    useState<ProjectComposition | null>(null);
  const [currentRawMicAudioPath, rawSetCurrentRawMicAudioPath] = useState("");
  const [currentRawWebcamVideoPath, rawSetCurrentRawWebcamVideoPath] =
    useState("");

  const segmentRef = useRef<VideoSegment | null>(null);
  const onProjectLoadedRef = useRef<(project: Project) => void>(null!);
  const currentProjectIdRef = useRef<string | null>(null);
  const currentProjectDataRef = useRef<Project | null>(null);
  const compositionRef = useRef<ProjectComposition | null>(null);
  const isPlayingRef = useRef(false);
  const pendingSilentSegmentRef = useRef<VideoSegment | null>(null);
  const pendingSilentSegmentTimerRef = useRef<number | null>(null);
  const persistRef = useRef<
    ((options?: PersistOptions) => Promise<void>) | null
  >(null);
  const isProjectTransitionRef = useRef(false);

  const setCurrentProjectData = useCallback<Dispatch<SetStateAction<Project | null>>>(
    (value) => {
      const next = typeof value === "function"
        ? value(currentProjectDataRef.current)
        : value;
      currentProjectDataRef.current = next;
      compositionRef.current = next?.composition ?? null;
      setCurrentProjectDataState(next);
      setCompositionState(compositionRef.current);
    },
    [],
  );

  const rawSetComposition = useCallback<Dispatch<SetStateAction<ProjectComposition | null>>>(
    (value) => {
      const next = typeof value === "function"
        ? value(compositionRef.current)
        : value;
      compositionRef.current = next;
      setCompositionState(next);
      const currentProject = currentProjectDataRef.current;
      if (!currentProject) return;
      const nextProject = {
        ...currentProject,
        composition: next ?? undefined,
      };
      currentProjectDataRef.current = nextProject;
      setCurrentProjectDataState(nextProject);
    },
    [],
  );

  return {
    composition,
    currentProjectData,
    currentProjectDataRef,
    currentProjectIdRef,
    currentRawMicAudioPath,
    currentRawWebcamVideoPath,
    currentRecordingMode,
    isPlaceholderBackedProject: Boolean(
      composition?.placeholderVideoForAudio ||
        composition?.placeholderVideoForSubtitles ||
        composition?.timelineOnly ||
        segment?.mediaMode === "timelineOnly",
    ),
    isPlayingRef,
    isProjectTransitionRef,
    isTimelineOnlyProject: Boolean(
      segment?.mediaMode === "timelineOnly" || composition?.timelineOnly,
    ),
    onProjectLoadedRef,
    pendingSilentSegmentRef,
    pendingSilentSegmentTimerRef,
    persistRef,
    rawSetComposition,
    rawSetCurrentRawMicAudioPath,
    rawSetCurrentRawWebcamVideoPath,
    rawSetCurrentRecordingMode,
    rawSetSegment,
    rawSetWebcamConfig,
    segment,
    segmentRef,
    setCurrentProjectData,
    webcamConfig,
  };
}
