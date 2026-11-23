import { act, renderHook } from "@testing-library/react";
import { StrictMode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  useEditorHistory,
  type EditorHistorySnapshot,
} from "@/hooks/useEditorHistory";

function createSnapshot(marker: number): EditorHistorySnapshot {
  return {
    segment: {
      subtitles: Array.from({ length: 1_000 }, (_, index) => ({
        end: index + 1,
        id: `${marker}-${index}`,
        start: index,
        text: `Subtitle ${index}`,
      })),
    } as unknown as EditorHistorySnapshot["segment"],
    composition: null,
    backgroundConfig: {} as EditorHistorySnapshot["backgroundConfig"],
    webcamConfig: {} as EditorHistorySnapshot["webcamConfig"],
    duration: 1_000,
    currentRecordingMode: "screen" as EditorHistorySnapshot["currentRecordingMode"],
    currentRawVideoPath: "video.mp4",
    currentRawMicAudioPath: "mic.wav",
    currentRawWebcamVideoPath: "webcam.mp4",
  };
}

describe("useEditorHistory", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("never clones the initial snapshot across view-only rerenders", () => {
    const cloneSpy = vi.spyOn(globalThis, "structuredClone");
    const initialSnapshot = createSnapshot(1);
    const { rerender } = renderHook(
      ({ snapshot }) => useEditorHistory({
        initialSnapshot: snapshot,
        applySnapshot: vi.fn(),
      }),
      { initialProps: { snapshot: initialSnapshot } },
    );

    expect(cloneSpy).not.toHaveBeenCalled();

    rerender({ snapshot: createSnapshot(2) });

    expect(cloneSpy).not.toHaveBeenCalled();
  });

  it("does not clone or push history for equivalent editor values", () => {
    const cloneSpy = vi.spyOn(globalThis, "structuredClone");
    const initialSnapshot = createSnapshot(1);
    const { result } = renderHook(() => useEditorHistory({
      initialSnapshot,
      applySnapshot: vi.fn(),
    }));

    act(() => {
      result.current.setSegment((previous) => previous);
      result.current.setComposition((previous) => previous);
      result.current.setBackgroundConfig({
        ...initialSnapshot.backgroundConfig,
      });
      result.current.setWebcamConfig({
        ...initialSnapshot.webcamConfig,
      });
      result.current.setDuration(initialSnapshot.duration);
      result.current.setCurrentRawVideoPath(
        initialSnapshot.currentRawVideoPath,
      );
    });

    expect(cloneSpy).not.toHaveBeenCalled();
    expect(result.current.canUndo).toBe(false);
  });

  it("records edits and batches without cloning or serializing heavy editor data", () => {
    const cloneSpy = vi.spyOn(globalThis, "structuredClone");
    const stringifySpy = vi.spyOn(JSON, "stringify");
    const initialSnapshot = createSnapshot(1);
    const { result } = renderHook(() => useEditorHistory({ initialSnapshot, applySnapshot: vi.fn() }));
    act(() => result.current.setSegment(createSnapshot(2).segment));
    act(() => result.current.beginBatch());
    act(() => result.current.setDuration(5));
    act(() => result.current.setDuration(1_000));
    act(() => result.current.commitBatch());
    act(() => result.current.beginBatch());
    act(() => result.current.setDuration(7));
    act(() => result.current.commitBatch());
    expect(cloneSpy).not.toHaveBeenCalled();
    expect(stringifySpy).not.toHaveBeenCalled();
    // The first batch returned to its starting value, so only two entries exist.
    act(() => result.current.undo());
    expect(result.current.getSnapshot().duration).toBe(1_000);
    act(() => result.current.undo());
    expect(result.current.getSnapshot().segment).toBe(initialSnapshot.segment);
    expect(result.current.canUndo).toBe(false);
  });

  it("undoes and redoes exactly one step per call under StrictMode", () => {
    const applySnapshot = vi.fn();
    const { result } = renderHook(
      () => useEditorHistory({ initialSnapshot: createSnapshot(1), applySnapshot }),
      { wrapper: StrictMode },
    );
    act(() => result.current.setDuration(1));
    act(() => result.current.setDuration(2));
    act(() => result.current.undo());
    expect(applySnapshot).toHaveBeenCalledTimes(1);
    expect(result.current.getSnapshot().duration).toBe(1);
    act(() => result.current.redo());
    expect(applySnapshot).toHaveBeenCalledTimes(2);
    expect(result.current.getSnapshot().duration).toBe(2);
    expect(result.current.canRedo).toBe(false);
    act(() => result.current.undo());
    act(() => result.current.undo());
    expect(result.current.getSnapshot().duration).toBe(1_000);
    expect(result.current.canUndo).toBe(false);
    expect(result.current.canRedo).toBe(true);
  });
});
