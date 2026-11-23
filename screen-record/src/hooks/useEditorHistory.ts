import { useCallback, useMemo, useRef, useState } from "react";
import { equalBackgroundConfig } from "@/lib/backgroundConfig";
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

function isPlainObject(value: unknown): value is Record<string, unknown> {
  if (typeof value !== "object" || value === null) return false;
  const prototype = Object.getPrototypeOf(value);
  return prototype === Object.prototype || prototype === null;
}

/**
 * Editor state is immutable and structurally shared, so equality short-circuits
 * on identical references and only walks the subtrees an edit replaced. Heavy
 * unchanged data (mouse positions, clip media) is never serialized or copied.
 */
function structurallyEqual(left: unknown, right: unknown): boolean {
  if (Object.is(left, right)) return true;
  if (Array.isArray(left)) {
    return Array.isArray(right) &&
      left.length === right.length &&
      left.every((item, index) => structurallyEqual(item, right[index]));
  }
  if (!isPlainObject(left) || !isPlainObject(right)) return false;
  const leftKeys = Object.keys(left).filter((key) => left[key] !== undefined);
  const rightKeys = Object.keys(right).filter((key) => right[key] !== undefined);
  return leftKeys.length === rightKeys.length &&
    leftKeys.every((key) => structurallyEqual(left[key], right[key]));
}

function snapshotsEqual(left: EditorHistorySnapshot, right: EditorHistorySnapshot) {
  return structurallyEqual(left, right);
}

function shallowObjectEqual<T extends object>(left: T, right: T) {
  if (left === right) return true;
  const leftKeys = Object.keys(left) as Array<keyof T>;
  const rightKeys = Object.keys(right) as Array<keyof T>;
  return leftKeys.length === rightKeys.length &&
    leftKeys.every((key) => Object.is(left[key], right[key]));
}

export function useEditorHistory({
  initialSnapshot,
  applySnapshot,
  maxHistory = 30,
}: UseEditorHistoryParams) {
  // History entries share references with immutable editor state; nothing is cloned.
  const [snapshotRef] = useState(() => ({ current: { ...initialSnapshot } }));
  const batchSnapshotRef = useRef<EditorHistorySnapshot | null>(null);
  const batchDepthRef = useRef(0);
  const suppressHistoryRef = useRef(0);
  // Stacks live in refs so undo/redo compute outside React updaters (which
  // StrictMode may double-invoke); the counts drive canUndo/canRedo renders.
  const pastRef = useRef<EditorHistorySnapshot[]>([]);
  const futureRef = useRef<EditorHistorySnapshot[]>([]);
  const [stackSizes, setStackSizes] = useState({ past: 0, future: 0 });
  const [isBatching, setIsBatching] = useState(false);

  const getSnapshot = useCallback(() => snapshotRef.current, [snapshotRef]);

  const replaceSnapshot = useCallback((patch: Partial<EditorHistorySnapshot>) => {
    snapshotRef.current = {
      ...snapshotRef.current,
      ...patch,
    };
  }, [snapshotRef]);

  const setStacks = useCallback((
    past: EditorHistorySnapshot[],
    future: EditorHistorySnapshot[],
  ) => {
    pastRef.current = past.length > maxHistory ? past.slice(past.length - maxHistory) : past;
    futureRef.current = future;
    setStackSizes((previous) =>
      previous.past === pastRef.current.length && previous.future === future.length
        ? previous
        : { past: pastRef.current.length, future: future.length });
  }, [maxHistory]);

  const pushPast = useCallback((snapshot: EditorHistorySnapshot) => {
    setStacks([...pastRef.current, snapshot], []);
  }, [setStacks]);

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
  }, [pushPast, snapshotRef]);

  const resetHistory = useCallback((snapshot: EditorHistorySnapshot) => {
    snapshotRef.current = { ...snapshot };
    batchSnapshotRef.current = null;
    batchDepthRef.current = 0;
    setIsBatching(false);
    setStacks([], []);
  }, [setStacks, snapshotRef]);

  const restore = useCallback((
    target: EditorHistorySnapshot,
    nextPast: EditorHistorySnapshot[],
    nextFuture: EditorHistorySnapshot[],
  ) => {
    setStacks(nextPast, nextFuture);
    snapshotRef.current = target;
    withoutHistory(() => applySnapshot(target));
  }, [applySnapshot, setStacks, snapshotRef, withoutHistory]);

  // Undo/redo inside an open gesture batch (Ctrl+Z mid-drag) first closes the
  // part of the gesture already made as its own step. The batch stays open and
  // captures a fresh baseline on its next change, so its commit cannot push a
  // stale pre-undo snapshot over the restored state.
  const settleOpenBatch = useCallback(() => {
    if (batchDepthRef.current === 0) return;
    const snapshot = batchSnapshotRef.current;
    batchSnapshotRef.current = null;
    if (snapshot && !snapshotsEqual(snapshot, snapshotRef.current)) pushPast(snapshot);
  }, [pushPast, snapshotRef]);

  const undo = useCallback(() => {
    settleOpenBatch();
    const past = pastRef.current;
    if (past.length === 0) return;
    const previous = past[past.length - 1];
    restore(previous, past.slice(0, -1), [getSnapshot(), ...futureRef.current]);
  }, [getSnapshot, restore, settleOpenBatch]);

  const redo = useCallback(() => {
    settleOpenBatch();
    const future = futureRef.current;
    if (future.length === 0) return;
    restore(future[0], [...pastRef.current, getSnapshot()], future.slice(1));
  }, [getSnapshot, restore, settleOpenBatch]);

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
    if (next === previous || equalBackgroundConfig(next, previous)) return;
    recordBeforeChange();
    replaceSnapshot({ backgroundConfig: next });
  }, [recordBeforeChange, replaceSnapshot]);

  const setWebcamConfig: Setter<WebcamConfig> = useCallback((value) => {
    const previous = snapshotRef.current.webcamConfig;
    const next = typeof value === "function"
      ? (value as (prev: WebcamConfig) => WebcamConfig)(previous)
      : value;
    if (next === previous || shallowObjectEqual(next, previous)) return;
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
    canUndo: stackSizes.past > 0,
    canRedo: stackSizes.future > 0,
    isBatching,
    beginBatch,
    commitBatch,
  }), [
    beginBatch,
    commitBatch,
    getSnapshot,
    isBatching,
    stackSizes,
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
