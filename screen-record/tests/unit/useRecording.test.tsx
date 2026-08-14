import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useRecording, type UseRecordingProps } from "@/hooks/useRecording";
import { invoke } from "@/lib/ipc";

vi.mock("@/lib/ipc", () => ({ invoke: vi.fn() }));
vi.mock("@/lib/videoRenderer", () => ({ videoRenderer: {} }));
vi.mock("@/lib/videoController", () => ({ createVideoController: vi.fn() }));
vi.mock("@/lib/autoZoom", () => ({ autoZoomGenerator: {} }));

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((nextResolve) => {
    resolve = nextResolve;
  });
  return { promise, resolve };
}

function makeProps(): UseRecordingProps {
  return {
    videoControllerRef: { current: undefined },
    videoRef: { current: document.createElement("video") },
    canvasRef: { current: document.createElement("canvas") },
    tempCanvasRef: { current: document.createElement("canvas") },
    backgroundConfig: {} as UseRecordingProps["backgroundConfig"],
    setSegment: vi.fn(),
    setCurrentVideo: vi.fn(),
    setCurrentAudio: vi.fn(),
    setCurrentMicAudio: vi.fn(),
    setCurrentWebcamVideo: vi.fn(),
    setIsVideoReady: vi.fn(),
    setThumbnails: vi.fn(),
    invalidateThumbnails: vi.fn(),
    setDuration: vi.fn(),
    setCurrentTime: vi.fn(),
    generateThumbnailsForSource: vi.fn(async () => undefined),
    generateThumbnail: vi.fn(),
    renderFrame: vi.fn(),
    currentVideo: "blob:existing-preview",
    currentAudio: null,
    currentMicAudio: null,
    currentWebcamVideo: null,
  };
}

describe("useRecording", () => {
  beforeEach(() => vi.mocked(invoke).mockReset());

  it("coalesces concurrent recording starts into one native request", async () => {
    const start = deferred<unknown>();
    vi.mocked(invoke).mockReturnValue(start.promise);
    const { result } = renderHook(() => useRecording(makeProps()));

    let first!: Promise<void>;
    let second!: Promise<void>;
    act(() => {
      first = result.current.startNewRecording("0", "withoutCursor");
      second = result.current.startNewRecording("0", "withoutCursor");
    });

    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith("start_recording", expect.any(Object));
    start.resolve(undefined);
    await act(async () => Promise.all([first, second]));
    expect(result.current.isRecording).toBe(true);
  });
});
