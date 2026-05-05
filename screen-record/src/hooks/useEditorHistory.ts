import { useCallback, useMemo, useRef, useState } from "react";
import type {
  BackgroundConfig,
  ProjectComposition,
  RecordingMode,
  VideoSegment,
  WebcamConfig,
} from "@/types/video";

export interface EditorHistorySnapshot {
  segment: VideoSegment | null;
  composition: ProjectComposition | null;
  backgroundConfig: BackgroundConfig;
  webcamConfig: WebcamConfig;
  duration: number;
  currentRecordingMode: RecordingMode;
  currentRawVideoPath: string;
  currentRawMicAudioPath: string;
  currentRawWebcamVideoPath: string;
}

type Setter<T> = (value: T | ((prev: T) => T)) => void;
type NullableSetter<T> = (value: T | null | ((prev: T | null) => T | null)) => void;

interface UseEditorHistoryParams {
  initialSnapshot: EditorHistorySnapshot;
  applySnapshot: (snapshot: EditorHistorySnapshot) => void;
  maxHistory?: number;
}

function cloneSnapshot(snapshot: EditorHistorySnapshot): EditorHistorySnapshot {
  return structuredClone(snapshot);
}

function snapshotsEqual(left: EditorHistorySnapshot, right: EditorHistorySnapshot) {
  return JSON.stringify(left) === JSON.stringify(right);
}

export function useEditorHistory({
  initialSnapshot,
  applySnapshot,
  maxHistory = 30,
}: UseEditorHistoryParams) {
  const snapshotRef = useRef<EditorHistorySnapshot>(cloneSnapshot(initialSnapshot));
  const batchSnapshotRef = useRef<EditorHistorySnapshot | null>(null);
  const batchDepthRef = useRef(0);
  const suppressHistoryRef = useRef(0);
  const [past, setPast] = useState<EditorHistorySnapshot[]>([]);
  const [future, setFuture] = useState<EditorHistorySnapshot[]>([]);
  const [isBatching, setIsBatching] = useState(false);

  const getSnapshot = useCallback(() => cloneSnapshot(snapshotRef.current), []);

  const replaceSnapshot = useCallback((patch: Partial<EditorHistorySnapshot>) => {
    snapshotRef.current = {
      ...snapshotRef.current,
      ...patch,
    };
  }, []);

  const pushPast = useCallback((snapshot: EditorHistorySnapshot) => {
    setPast((prev) => {
      const next = [...prev, cloneSnapshot(snapshot)];
      if (next.length > maxHistory) next.shift();
      return next;
    });
    setFuture([]);
  }, [maxHistory]);

  const recordBeforeChange = useCallback(() => {
    if (suppressHistoryRef.current > 0) return;
    if (batchDepthRef.current > 0) {
      if (!batchSnapshotRef.current) {
        batchSnapshotRef.current = getSnapshot();
      }
      return;
    }
    pushPast(getSnapshot());
  }, [getSnapshot, pushPast]);

  const withoutHistory = useCallback((fn: () => void) => {
    suppressHistoryRef.current += 1;
    try {
      fn();
    } finally {
      suppressHistoryRef.current = Math.max(0, suppressHistoryRef.current - 1);
    }
  }, []);

  const beginBatch = useCallback(() => {
    if (suppressHistoryRef.current > 0) return;
    batchDepthRef.current += 1;
    if (batchDepthRef.current === 1) {
      batchSnapshotRef.current = getSnapshot();
      setIsBatching(true);
    }
  }, [getSnapshot]);

  const commitBatch = useCallback(() => {
    if (batchDepthRef.current <= 0) return;
    batchDepthRef.current -= 1;
    if (batchDepthRef.current > 0) return;
    const snapshot = batchSnapshotRef.current;
    batchSnapshotRef.current = null;
    setIsBatching(false);
    if (!snapshot || snapshotsEqual(snapshot, snapshotRef.current)) return;
    pushPast(snapshot);
  }, [pushPast]);

  const resetHistory = useCallback((snapshot: EditorHistorySnapshot) => {
    snapshotRef.current = cloneSnapshot(snapshot);
    batchSnapshotRef.current = null;
    batchDepthRef.current = 0;
    setIsBatching(false);
    setPast([]);
    setFuture([]);
  }, []);

  const undo = useCallback(() => {
    setPast((prev) => {
      if (prev.length === 0) return prev;
      const previous = prev[prev.length - 1];
      const nextPast = prev.slice(0, -1);
      const current = getSnapshot();
      setFuture((nextFuture) => [current, ...nextFuture]);
      snapshotRef.current = cloneSnapshot(previous);
      withoutHistory(() => applySnapshot(previous));
      return nextPast;
    });
  }, [applySnapshot, getSnapshot, withoutHistory]);

  const redo = useCallback(() => {
    setFuture((prev) => {
      if (prev.length === 0) return prev;
      const next = prev[0];
      const nextFuture = prev.slice(1);
      const current = getSnapshot();
      setPast((nextPast) => {
        const updated = [...nextPast, current];
        if (updated.length > maxHistory) updated.shift();
        return updated;
      });
      snapshotRef.current = cloneSnapshot(next);
      withoutHistory(() => applySnapshot(next));
      return nextFuture;
    });
  }, [applySnapshot, getSnapshot, maxHistory, withoutHistory]);

  const setSegment: NullableSetter<VideoSegment> = useCallback((value) => {
    const previous = snapshotRef.current.segment;
    const next = typeof value === "function"
      ? (value as (prev: VideoSegment | null) => VideoSegment | null)(previous)
      : value;
    if (next === previous) return;
    recordBeforeChange();
    replaceSnapshot({ segment: next });
  }, [recordBeforeChange, replaceSnapshot]);

  const setComposition: NullableSetter<ProjectComposition> = useCallback((value) => {
    const previous = snapshotRef.current.composition;
    const next = typeof value === "function"
      ? (value as (prev: ProjectComposition | null) => ProjectComposition | null)(previous)
      : value;
    if (next === previous) return;
    recordBeforeChange();
    replaceSnapshot({ composition: next });
  }, [recordBeforeChange, replaceSnapshot]);

  const setBackgroundConfig: Setter<BackgroundConfig> = useCallback((value) => {
    const previous = snapshotRef.current.backgroundConfig;
    const next = typeof value === "function"
      ? (value as (prev: BackgroundConfig) => BackgroundConfig)(previous)
      : value;
    if (next === previous) return;
    recordBeforeChange();
    replaceSnapshot({ backgroundConfig: next });
  }, [recordBeforeChange, replaceSnapshot]);

  const setWebcamConfig: Setter<WebcamConfig> = useCallback((value) => {
    const previous = snapshotRef.current.webcamConfig;
    const next = typeof value === "function"
      ? (value as (prev: WebcamConfig) => WebcamConfig)(previous)
      : value;
    if (next === previous) return;
    recordBeforeChange();
    replaceSnapshot({ webcamConfig: next });
  }, [recordBeforeChange, replaceSnapshot]);

  const setDuration = useCallback((duration: number) => {
    if (duration === snapshotRef.current.duration) return;
    recordBeforeChange();
    replaceSnapshot({ duration });
  }, [recordBeforeChange, replaceSnapshot]);

  const setCurrentRecordingMode = useCallback((currentRecordingMode: RecordingMode) => {
    if (currentRecordingMode === snapshotRef.current.currentRecordingMode) return;
    recordBeforeChange();
    replaceSnapshot({ currentRecordingMode });
  }, [recordBeforeChange, replaceSnapshot]);

  const setCurrentRawVideoPath = useCallback((currentRawVideoPath: string) => {
    if (currentRawVideoPath === snapshotRef.current.currentRawVideoPath) return;
    recordBeforeChange();
    replaceSnapshot({ currentRawVideoPath });
  }, [recordBeforeChange, replaceSnapshot]);

  const setCurrentRawMicAudioPath = useCallback((currentRawMicAudioPath: string) => {
    if (currentRawMicAudioPath === snapshotRef.current.currentRawMicAudioPath) return;
    recordBeforeChange();
    replaceSnapshot({ currentRawMicAudioPath });
  }, [recordBeforeChange, replaceSnapshot]);

  const setCurrentRawWebcamVideoPath = useCallback((currentRawWebcamVideoPath: string) => {
    if (currentRawWebcamVideoPath === snapshotRef.current.currentRawWebcamVideoPath) return;
    recordBeforeChange();
    replaceSnapshot({ currentRawWebcamVideoPath });
  }, [recordBeforeChange, replaceSnapshot]);

  return useMemo(() => ({
    getSnapshot,
    resetHistory,
    withoutHistory,
    replaceSnapshot,
    setSegment,
    setComposition,
    setBackgroundConfig,
    setWebcamConfig,
    setDuration,
    setCurrentRecordingMode,
    setCurrentRawVideoPath,
    setCurrentRawMicAudioPath,
    setCurrentRawWebcamVideoPath,
    undo,
    redo,
    canUndo: past.length > 0,
    canRedo: future.length > 0,
    isBatching,
    beginBatch,
    commitBatch,
  }), [
    beginBatch,
    commitBatch,
    future.length,
    getSnapshot,
    isBatching,
    past.length,
    redo,
    replaceSnapshot,
    resetHistory,
    setBackgroundConfig,
    setComposition,
    setCurrentRawMicAudioPath,
    setCurrentRawVideoPath,
    setCurrentRawWebcamVideoPath,
    setCurrentRecordingMode,
    setDuration,
    setSegment,
    setWebcamConfig,
    undo,
    withoutHistory,
  ]);
}
