import { act, renderHook } from "@testing-library/react";
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

  it("clones the initial snapshot only once across view-only rerenders", () => {
    const cloneSpy = vi.spyOn(globalThis, "structuredClone");
    const initialSnapshot = createSnapshot(1);
    const { rerender } = renderHook(
      ({ snapshot }) => useEditorHistory({
        initialSnapshot: snapshot,
        applySnapshot: vi.fn(),
      }),
      { initialProps: { snapshot: initialSnapshot } },
    );

    expect(cloneSpy).toHaveBeenCalledTimes(1);

    rerender({ snapshot: createSnapshot(2) });

    expect(cloneSpy).toHaveBeenCalledTimes(1);
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

    expect(cloneSpy).toHaveBeenCalledTimes(1);
    expect(result.current.canUndo).toBe(false);
  });
});
