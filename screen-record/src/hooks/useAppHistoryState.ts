import {
  useCallback,
  useEffect,
  useRef,
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
import {
  cloneBackgroundConfig,
  equalBackgroundConfig,
} from "@/lib/backgroundConfig";
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
  onHistoryRestore?: () => void;
  backgroundConfig: BackgroundConfig;
  backgroundConfigRef: MutableRefObject<BackgroundConfig>;
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
  isProjectTransitionRef: MutableRefObject<boolean>;
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
  webcamConfigRef: MutableRefObject<WebcamConfig>;
}

function preserveSilentAudioLanes(
  nextComposition: ProjectComposition | null,
  previousComposition: ProjectComposition | null | undefined,
  projectComposition: ProjectComposition | null | undefined,
) {
  if (!nextComposition) return nextComposition;
  const fallbackComposition = projectComposition ?? previousComposition;
  if (!fallbackComposition) return nextComposition;
  const audioSegments =
    nextComposition.audioSegments === undefined &&
    fallbackComposition.audioSegments !== undefined
      ? fallbackComposition.audioSegments
      : nextComposition.audioSegments;
  const audioTrackVolumePoints =
    nextComposition.audioTrackVolumePoints === undefined &&
    fallbackComposition.audioTrackVolumePoints !== undefined
      ? fallbackComposition.audioTrackVolumePoints
      : nextComposition.audioTrackVolumePoints;
  const narrationSegments =
    nextComposition.narrationSegments === undefined &&
    fallbackComposition.narrationSegments !== undefined
      ? fallbackComposition.narrationSegments
      : nextComposition.narrationSegments;
  const narrationTrackVolumePoints =
    nextComposition.narrationTrackVolumePoints === undefined &&
    fallbackComposition.narrationTrackVolumePoints !== undefined
      ? fallbackComposition.narrationTrackVolumePoints
      : nextComposition.narrationTrackVolumePoints;
  const retainedSource = nextComposition.retainedRemovedClips ??
    fallbackComposition.retainedRemovedClips;
  const activeClipIds = new Set(nextComposition.clips.map((clip) => clip.id));
  const filteredRetained = retainedSource?.filter(
    (retained) => !activeClipIds.has(retained.id),
  );
  const retainedRemovedClips =
    retainedSource && filteredRetained?.length === retainedSource.length
      ? retainedSource
      : filteredRetained;

  if (
    audioSegments === nextComposition.audioSegments &&
    audioTrackVolumePoints === nextComposition.audioTrackVolumePoints &&
    narrationSegments === nextComposition.narrationSegments &&
    narrationTrackVolumePoints === nextComposition.narrationTrackVolumePoints &&
    retainedRemovedClips === nextComposition.retainedRemovedClips
  ) {
    return nextComposition;
  }
  return {
    ...nextComposition,
    audioSegments,
    audioTrackVolumePoints,
    narrationSegments,
    narrationTrackVolumePoints,
    retainedRemovedClips,
  };
}

function shallowObjectEqual<T extends object>(left: T, right: T) {
  if (left === right) return true;
  const leftKeys = Object.keys(left) as Array<keyof T>;
  const rightKeys = Object.keys(right) as Array<keyof T>;
  return leftKeys.length === rightKeys.length &&
    leftKeys.every((key) => Object.is(left[key], right[key]));
}

function isRootClip(composition: ProjectComposition | null | undefined): boolean {
  if (!composition || composition.clips.length <= 1) return true;
  return (composition.focusedClipId ?? composition.selectedClipId ?? "root") === "root";
}

function applyCompositionToProject(
  project: Project,
  composition: ProjectComposition | null,
): Project {
  return {
    ...project,
    composition: composition ?? undefined,
  };
}

export function useAppHistoryState({
  onHistoryRestore,
  backgroundConfig,
  backgroundConfigRef,
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
  isProjectTransitionRef,
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
  webcamConfigRef,
}: UseAppHistoryStateOptions) {
  const visualTransitionRef = useRef<MutableRefObject<boolean> | null>(null);
  const bindVisualTransition = useCallback((ref: MutableRefObject<boolean>) => {
    visualTransitionRef.current = ref;
  }, []);
  const isVisualTransition = useCallback(() =>
    isProjectTransitionRef.current || visualTransitionRef.current?.current === true,
  [isProjectTransitionRef]);
  const applyHistorySnapshot = useCallback((snapshot: EditorHistorySnapshot) => {
    if (pendingSilentSegmentTimerRef.current !== null) {
      window.clearTimeout(pendingSilentSegmentTimerRef.current);
      pendingSilentSegmentTimerRef.current = null;
    }
    pendingSilentSegmentRef.current = null;
    segmentRef.current = snapshot.segment;
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
      backgroundConfig: isRootClip(snapshot.composition)
        ? cloneBackgroundConfig(snapshot.backgroundConfig) : project.backgroundConfig,
      composition: snapshot.composition ?? undefined,
      duration: snapshot.duration,
      rawMicAudioPath: snapshot.currentRawMicAudioPath || undefined,
      rawVideoPath: snapshot.currentRawVideoPath || undefined,
      rawWebcamVideoPath: snapshot.currentRawWebcamVideoPath || undefined,
      segment: snapshot.segment ?? project.segment,
      webcamConfig: isRootClip(snapshot.composition)
        ? cloneWebcamConfig(snapshot.webcamConfig) : project.webcamConfig,
    });
    currentProjectDataRef.current = currentProjectDataRef.current
      ? applyProjectSnapshot(currentProjectDataRef.current)
      : currentProjectDataRef.current;
    setCurrentProjectData((prev) => prev ? applyProjectSnapshot(prev) : prev);
  }, [
    currentProjectDataRef,
    pendingSilentSegmentRef,
    pendingSilentSegmentTimerRef,
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
    segmentRef,
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
    undo: undoHistory,
    redo: redoHistory,
    canUndo,
    canRedo,
    isBatching,
    beginBatch: beginHistoryBatch,
    commitBatch,
  } = editorHistory;
  const undo = useCallback(() => {
    if (!canUndo || isVisualTransition()) return;
    onHistoryRestore?.();
    undoHistory();
  }, [canUndo, isVisualTransition, onHistoryRestore, undoHistory]);
  const redo = useCallback(() => {
    if (!canRedo || isVisualTransition()) return;
    onHistoryRestore?.();
    redoHistory();
  }, [canRedo, isVisualTransition, onHistoryRestore, redoHistory]);
  const beginBatch = useCallback(() => {
    editorHistory.replaceSnapshot({
      backgroundConfig: backgroundConfigRef.current,
      webcamConfig: webcamConfigRef.current,
      ...(currentProjectDataRef.current
        ? { composition: currentProjectDataRef.current.composition ?? null } : {}),
    });
    beginHistoryBatch();
  }, [backgroundConfigRef, beginHistoryBatch, currentProjectDataRef, editorHistory, webcamConfigRef]);

  const setSegment = useCallback<SegmentSetter>((value) => {
    const baseSegment =
      segmentRef.current ??
      currentProjectDataRef.current?.segment ??
      segment;
    const nextSegment = typeof value === "function"
      ? (value as (current: VideoSegment | null) => VideoSegment | null)(baseSegment)
      : value;
    if (nextSegment === baseSegment) return;
    if (pendingSilentSegmentTimerRef.current !== null) {
      window.clearTimeout(pendingSilentSegmentTimerRef.current);
      pendingSilentSegmentTimerRef.current = null;
    }
    pendingSilentSegmentRef.current = null;
    editorHistory.setSegment(nextSegment);
    segmentRef.current = nextSegment;
    rawSetSegment(nextSegment);
    if (nextSegment && currentProjectDataRef.current) {
      currentProjectDataRef.current = {
        ...currentProjectDataRef.current,
        segment: nextSegment,
      };
      setCurrentProjectData(currentProjectDataRef.current);
    }
  }, [
    currentProjectDataRef,
    editorHistory,
    pendingSilentSegmentRef,
    pendingSilentSegmentTimerRef,
    rawSetSegment,
    segment,
    segmentRef,
    setCurrentProjectData,
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
    if (nextSegment === baseSegment) return;
    segmentRef.current = nextSegment;
    editorHistory.replaceSnapshot({ segment: nextSegment });
    if (nextSegment && currentProjectDataRef.current) {
      currentProjectDataRef.current = {
        ...currentProjectDataRef.current,
        segment: nextSegment,
      };
      setCurrentProjectData(currentProjectDataRef.current);
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
    editorHistory,
    flushPendingSilentSegment,
    isPlayingRef,
    pendingSilentSegmentRef,
    pendingSilentSegmentTimerRef,
    rawSetSegment,
    segment,
    segmentRef,
    setCurrentProjectData,
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
    const previous = currentProjectDataRef.current
      ? currentProjectDataRef.current.composition ?? null
      : composition;
    const next = preserveSilentAudioLanes(
      typeof value === "function" ? value(previous ?? null) : value,
      previous,
      currentProjectDataRef.current?.composition,
    );
    if (next === previous) return;
    editorHistory.setComposition(next);
    rawSetComposition(next);
    if (currentProjectDataRef.current) {
      currentProjectDataRef.current = applyCompositionToProject(
        currentProjectDataRef.current,
        next,
      );
      setCurrentProjectData(currentProjectDataRef.current);
    }
  }, [composition, currentProjectDataRef, editorHistory, rawSetComposition, setCurrentProjectData]);

  const setCompositionSilently = useCallback<CompositionSetter>((value) => {
    const previous = currentProjectDataRef.current
      ? currentProjectDataRef.current.composition ?? null
      : composition;
    const next = preserveSilentAudioLanes(
      typeof value === "function" ? value(previous ?? null) : value,
      previous,
      currentProjectDataRef.current?.composition,
    );
    if (next === previous) return;
    editorHistory.replaceSnapshot({ composition: next });
    rawSetComposition(next);
    if (currentProjectDataRef.current) {
      currentProjectDataRef.current = applyCompositionToProject(
        currentProjectDataRef.current,
        next,
      );
      setCurrentProjectData(currentProjectDataRef.current);
    }
  }, [composition, currentProjectDataRef, editorHistory, rawSetComposition, setCurrentProjectData]);

  const setEditorPreviewDuration = useCallback((value: number) => {
    const previous = currentProjectDataRef.current?.duration ?? duration;
    if (value === previous) return;
    editorHistory.setDuration(value);
    setPreviewDuration(value);
    if (currentProjectDataRef.current) {
      currentProjectDataRef.current = {
        ...currentProjectDataRef.current,
        duration: value,
      };
      setCurrentProjectData(currentProjectDataRef.current);
    }
  }, [currentProjectDataRef, duration, editorHistory, setCurrentProjectData, setPreviewDuration]);

  const handleEditorRawVideoPathChange = useCallback((value: string) => {
    const previous = currentProjectDataRef.current?.rawVideoPath ?? currentRawVideoPath;
    if (value === previous) return;
    editorHistory.setCurrentRawVideoPath(value);
    handleProjectRawVideoPathChange(value);
    if (currentProjectDataRef.current) {
      currentProjectDataRef.current = {
        ...currentProjectDataRef.current,
        rawVideoPath: value || undefined,
      };
      setCurrentProjectData(currentProjectDataRef.current);
    }
  }, [currentProjectDataRef, currentRawVideoPath, editorHistory, handleProjectRawVideoPathChange, setCurrentProjectData]);

  const setBackgroundConfig = useCallback((
    value: BackgroundConfig | ((prev: BackgroundConfig) => BackgroundConfig),
  ) => {
    if (isVisualTransition()) return;
    const previous = backgroundConfigRef.current;
    const next = typeof value === "function" ? value(previous) : value;
    if (next === previous || equalBackgroundConfig(next, previous)) return;
    editorHistory.replaceSnapshot({ backgroundConfig: previous, webcamConfig: webcamConfigRef.current,
      ...(currentProjectDataRef.current
        ? { composition: currentProjectDataRef.current.composition ?? null } : {}) });
    editorHistory.setBackgroundConfig(next);
    rawSetBackgroundConfig(next);
    if (currentProjectDataRef.current && isRootClip(currentProjectDataRef.current.composition)) {
      currentProjectDataRef.current = {
        ...currentProjectDataRef.current,
        backgroundConfig: next,
      };
      setCurrentProjectData(currentProjectDataRef.current);
    }
  }, [backgroundConfigRef, currentProjectDataRef, editorHistory, isVisualTransition, rawSetBackgroundConfig, setCurrentProjectData, webcamConfigRef]);

  const setWebcamConfig = useCallback((
    value: WebcamConfig | ((prev: WebcamConfig) => WebcamConfig),
  ) => {
    if (isVisualTransition()) return;
    const previous = webcamConfigRef.current;
    const next = typeof value === "function" ? value(previous) : value;
    if (next === previous || shallowObjectEqual(next, previous)) return;
    editorHistory.replaceSnapshot({ backgroundConfig: backgroundConfigRef.current, webcamConfig: previous,
      ...(currentProjectDataRef.current
        ? { composition: currentProjectDataRef.current.composition ?? null } : {}) });
    editorHistory.setWebcamConfig(next);
    rawSetWebcamConfig(next);
    if (currentProjectDataRef.current && isRootClip(currentProjectDataRef.current.composition)) {
      currentProjectDataRef.current = {
        ...currentProjectDataRef.current,
        webcamConfig: next,
      };
      setCurrentProjectData(currentProjectDataRef.current);
    }
  }, [backgroundConfigRef, currentProjectDataRef, editorHistory, isVisualTransition, rawSetWebcamConfig, setCurrentProjectData, webcamConfigRef]);

  useEffect(() => {
    editorHistory.replaceSnapshot({
      backgroundConfig,
      composition,
      currentRawMicAudioPath,
      currentRawVideoPath,
      currentRawWebcamVideoPath,
      currentRecordingMode,
      duration,
      segment: pendingSilentSegmentRef.current ?? segment,
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
    pendingSilentSegmentRef,
    segment,
    webcamConfig,
  ]);

  return {
    bindVisualTransition,
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
