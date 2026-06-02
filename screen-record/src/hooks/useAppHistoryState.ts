import {
  useCallback,
  useEffect,
  type Dispatch,
  type MutableRefObject,
  type SetStateAction,
} from "react";
import type {
  BackgroundConfig,
  Project,
  ProjectComposition,
  RecordingMode,
  VideoSegment,
  WebcamConfig,
} from "@/types/video";
import { useEditorHistory, type EditorHistorySnapshot } from "@/hooks/useEditorHistory";
import { cloneBackgroundConfig } from "@/lib/backgroundConfig";
import { cloneWebcamConfig } from "@/lib/webcam";

type CompositionSetter = (
  value:
    | ProjectComposition
    | null
    | ((prev: ProjectComposition | null) => ProjectComposition | null),
) => void;

type SegmentSetter = (
  value:
    | VideoSegment
    | null
    | ((prev: VideoSegment | null) => VideoSegment | null),
) => void;

interface UseAppHistoryStateOptions {
  backgroundConfig: BackgroundConfig;
  composition: ProjectComposition | null;
  currentProjectDataRef: MutableRefObject<Project | null>;
  currentRawMicAudioPath: string;
  currentRawVideoPath: string;
  currentRawWebcamVideoPath: string;
  currentRecordingMode: RecordingMode;
  duration: number;
  handleProjectRawVideoPathChange: (value: string) => void;
  isPlaying: boolean;
  isPlayingRef: MutableRefObject<boolean>;
  pendingSilentSegmentRef: MutableRefObject<VideoSegment | null>;
  pendingSilentSegmentTimerRef: MutableRefObject<number | null>;
  rawSetBackgroundConfig: (
    value: BackgroundConfig | ((prev: BackgroundConfig) => BackgroundConfig),
  ) => void;
  rawSetComposition: Dispatch<SetStateAction<ProjectComposition | null>>;
  rawSetCurrentRawMicAudioPath: Dispatch<SetStateAction<string>>;
  rawSetCurrentRawVideoPath: (value: string) => void;
  rawSetCurrentRawWebcamVideoPath: Dispatch<SetStateAction<string>>;
  rawSetCurrentRecordingMode: Dispatch<SetStateAction<RecordingMode>>;
  rawSetSegment: Dispatch<SetStateAction<VideoSegment | null>>;
  rawSetWebcamConfig: Dispatch<SetStateAction<WebcamConfig>>;
  segment: VideoSegment | null;
  segmentRef: MutableRefObject<VideoSegment | null>;
  setBackgroundConfigState: Dispatch<SetStateAction<BackgroundConfig>>;
  setCurrentProjectData: Dispatch<SetStateAction<Project | null>>;
  setLastRawSavedPath: (value: string) => void;
  setPreviewDuration: Dispatch<SetStateAction<number>>;
  webcamConfig: WebcamConfig;
}

function preserveSilentAudioLanes(
  nextComposition: ProjectComposition | null,
  previousComposition: ProjectComposition | null | undefined,
  projectComposition: ProjectComposition | null | undefined,
) {
  if (!nextComposition) return nextComposition;
  const fallbackComposition = projectComposition ?? previousComposition;
  if (!fallbackComposition) return nextComposition;
  return {
    ...nextComposition,
    audioSegments:
      (nextComposition.audioSegments?.length ?? 0) === 0 &&
      (fallbackComposition.audioSegments?.length ?? 0) > 0
        ? fallbackComposition.audioSegments
        : nextComposition.audioSegments,
    audioTrackVolumePoints:
      (nextComposition.audioTrackVolumePoints?.length ?? 0) === 0 &&
      (fallbackComposition.audioTrackVolumePoints?.length ?? 0) > 0
        ? fallbackComposition.audioTrackVolumePoints
        : nextComposition.audioTrackVolumePoints,
    narrationSegments:
      (nextComposition.narrationSegments?.length ?? 0) === 0 &&
      (fallbackComposition.narrationSegments?.length ?? 0) > 0
        ? fallbackComposition.narrationSegments
        : nextComposition.narrationSegments,
    narrationTrackVolumePoints:
      (nextComposition.narrationTrackVolumePoints?.length ?? 0) === 0 &&
      (fallbackComposition.narrationTrackVolumePoints?.length ?? 0) > 0
        ? fallbackComposition.narrationTrackVolumePoints
        : nextComposition.narrationTrackVolumePoints,
  };
}

export function useAppHistoryState({
  backgroundConfig,
  composition,
  currentProjectDataRef,
  currentRawMicAudioPath,
  currentRawVideoPath,
  currentRawWebcamVideoPath,
  currentRecordingMode,
  duration,
  handleProjectRawVideoPathChange,
  isPlaying,
  isPlayingRef,
  pendingSilentSegmentRef,
  pendingSilentSegmentTimerRef,
  rawSetBackgroundConfig,
  rawSetComposition,
  rawSetCurrentRawMicAudioPath,
  rawSetCurrentRawVideoPath,
  rawSetCurrentRawWebcamVideoPath,
  rawSetCurrentRecordingMode,
  rawSetSegment,
  rawSetWebcamConfig,
  segment,
  segmentRef,
  setBackgroundConfigState,
  setCurrentProjectData,
  setLastRawSavedPath,
  setPreviewDuration,
  webcamConfig,
}: UseAppHistoryStateOptions) {
  const applyHistorySnapshot = useCallback((snapshot: EditorHistorySnapshot) => {
    rawSetSegment(snapshot.segment);
    rawSetComposition(snapshot.composition);
    setBackgroundConfigState(cloneBackgroundConfig(snapshot.backgroundConfig));
    rawSetWebcamConfig(cloneWebcamConfig(snapshot.webcamConfig));
    setPreviewDuration(snapshot.duration);
    rawSetCurrentRecordingMode(snapshot.currentRecordingMode);
    rawSetCurrentRawVideoPath(snapshot.currentRawVideoPath);
    setLastRawSavedPath("");
    rawSetCurrentRawMicAudioPath(snapshot.currentRawMicAudioPath);
    rawSetCurrentRawWebcamVideoPath(snapshot.currentRawWebcamVideoPath);
    const applyProjectSnapshot = (project: Project): Project => ({
      ...project,
      backgroundConfig: cloneBackgroundConfig(snapshot.backgroundConfig),
      composition: snapshot.composition ?? undefined,
      duration: snapshot.duration,
      rawMicAudioPath: snapshot.currentRawMicAudioPath || undefined,
      rawVideoPath: snapshot.currentRawVideoPath || undefined,
      rawWebcamVideoPath: snapshot.currentRawWebcamVideoPath || undefined,
      segment: snapshot.segment ?? project.segment,
      webcamConfig: cloneWebcamConfig(snapshot.webcamConfig),
    });
    currentProjectDataRef.current = currentProjectDataRef.current
      ? applyProjectSnapshot(currentProjectDataRef.current)
      : currentProjectDataRef.current;
    setCurrentProjectData((prev) => prev ? applyProjectSnapshot(prev) : prev);
  }, [
    currentProjectDataRef,
    rawSetComposition,
    rawSetCurrentRawMicAudioPath,
    rawSetCurrentRawVideoPath,
    rawSetCurrentRawWebcamVideoPath,
    rawSetCurrentRecordingMode,
    rawSetSegment,
    rawSetWebcamConfig,
    setBackgroundConfigState,
    setCurrentProjectData,
    setLastRawSavedPath,
    setPreviewDuration,
  ]);

  const editorHistory = useEditorHistory({
    initialSnapshot: {
      backgroundConfig,
      composition,
      currentRawMicAudioPath,
      currentRawVideoPath,
      currentRawWebcamVideoPath,
      currentRecordingMode,
      duration,
      segment,
      webcamConfig,
    },
    applySnapshot: applyHistorySnapshot,
  });
  const {
    undo,
    redo,
    canUndo,
    canRedo,
    isBatching,
    beginBatch,
    commitBatch,
  } = editorHistory;

  const setSegment = useCallback<SegmentSetter>((value) => {
    if (pendingSilentSegmentTimerRef.current !== null) {
      window.clearTimeout(pendingSilentSegmentTimerRef.current);
      pendingSilentSegmentTimerRef.current = null;
    }
    pendingSilentSegmentRef.current = null;
    const baseSegment =
      segmentRef.current ??
      currentProjectDataRef.current?.segment ??
      segment;
    const nextSegment = typeof value === "function"
      ? (value as (current: VideoSegment | null) => VideoSegment | null)(baseSegment)
      : value;
    editorHistory.setSegment(nextSegment);
    rawSetSegment(nextSegment);
    if (nextSegment && currentProjectDataRef.current) {
      currentProjectDataRef.current = {
        ...currentProjectDataRef.current,
        segment: nextSegment,
      };
    }
  }, [
    currentProjectDataRef,
    editorHistory,
    pendingSilentSegmentRef,
    pendingSilentSegmentTimerRef,
    rawSetSegment,
    segment,
    segmentRef,
  ]);

  const flushPendingSilentSegment = useCallback(() => {
    pendingSilentSegmentTimerRef.current = null;
    const nextSegment = pendingSilentSegmentRef.current;
    pendingSilentSegmentRef.current = null;
    rawSetSegment(nextSegment);
  }, [pendingSilentSegmentRef, pendingSilentSegmentTimerRef, rawSetSegment]);

  const setSegmentSilently = useCallback<SegmentSetter>((value) => {
    const baseSegment =
      segmentRef.current ??
      currentProjectDataRef.current?.segment ??
      segment;
    const nextSegment = typeof value === "function"
      ? (value as (current: VideoSegment | null) => VideoSegment | null)(baseSegment)
      : value;
    segmentRef.current = nextSegment;
    if (nextSegment && currentProjectDataRef.current) {
      currentProjectDataRef.current = {
        ...currentProjectDataRef.current,
        segment: nextSegment,
      };
    }
    if (!isPlayingRef.current) {
      rawSetSegment(nextSegment);
      return;
    }
    pendingSilentSegmentRef.current = nextSegment;
    if (pendingSilentSegmentTimerRef.current === null) {
      pendingSilentSegmentTimerRef.current = window.setTimeout(flushPendingSilentSegment, 300);
    }
  }, [
    currentProjectDataRef,
    flushPendingSilentSegment,
    isPlayingRef,
    pendingSilentSegmentRef,
    pendingSilentSegmentTimerRef,
    rawSetSegment,
    segment,
    segmentRef,
  ]);

  useEffect(() => {
    isPlayingRef.current = isPlaying;
    if (!isPlaying && pendingSilentSegmentRef.current) {
      if (pendingSilentSegmentTimerRef.current !== null) {
        window.clearTimeout(pendingSilentSegmentTimerRef.current);
      }
      flushPendingSilentSegment();
    }
  }, [
    flushPendingSilentSegment,
    isPlaying,
    isPlayingRef,
    pendingSilentSegmentRef,
    pendingSilentSegmentTimerRef,
  ]);

  const setComposition = useCallback<CompositionSetter>((value) => {
    editorHistory.setComposition(value);
    rawSetComposition(value);
  }, [editorHistory, rawSetComposition]);

  const setCompositionSilently = useCallback<CompositionSetter>((value) => {
    rawSetComposition((prev) => {
      const next = typeof value === "function"
        ? (value as (current: ProjectComposition | null) => ProjectComposition | null)(prev)
        : value;
      return preserveSilentAudioLanes(
        next,
        prev,
        currentProjectDataRef.current?.composition ?? null,
      );
    });
  }, [currentProjectDataRef, rawSetComposition]);

  const setEditorPreviewDuration = useCallback((value: number) => {
    editorHistory.setDuration(value);
    setPreviewDuration(value);
  }, [editorHistory, setPreviewDuration]);

  const handleEditorRawVideoPathChange = useCallback((value: string) => {
    editorHistory.setCurrentRawVideoPath(value);
    handleProjectRawVideoPathChange(value);
  }, [editorHistory, handleProjectRawVideoPathChange]);

  const setBackgroundConfig = useCallback((
    value: BackgroundConfig | ((prev: BackgroundConfig) => BackgroundConfig),
  ) => {
    editorHistory.setBackgroundConfig(value);
    rawSetBackgroundConfig(value);
  }, [editorHistory, rawSetBackgroundConfig]);

  const setWebcamConfig = useCallback((
    value: WebcamConfig | ((prev: WebcamConfig) => WebcamConfig),
  ) => {
    editorHistory.setWebcamConfig(value);
    rawSetWebcamConfig(value);
  }, [editorHistory, rawSetWebcamConfig]);

  useEffect(() => {
    editorHistory.replaceSnapshot({
      backgroundConfig,
      composition,
      currentRawMicAudioPath,
      currentRawVideoPath,
      currentRawWebcamVideoPath,
      currentRecordingMode,
      duration,
      segment,
      webcamConfig,
    });
  }, [
    backgroundConfig,
    composition,
    currentRawMicAudioPath,
    currentRawVideoPath,
    currentRawWebcamVideoPath,
    currentRecordingMode,
    duration,
    editorHistory,
    segment,
    webcamConfig,
  ]);

  return {
    beginBatch,
    canRedo,
    canUndo,
    commitBatch,
    editorHistory,
    handleEditorRawVideoPathChange,
    isBatching,
    redo,
    setBackgroundConfig,
    setComposition,
    setCompositionSilently,
    setEditorPreviewDuration,
    setSegment,
    setSegmentSilently,
    setWebcamConfig,
    undo,
  };
}
