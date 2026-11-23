import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useRecording, type UseRecordingProps } from "@/hooks/useRecording";
import { invoke } from "@/lib/ipc";
import { recordingHandoff } from "@/lib/recordingHandoff";
import { beginAudioImportShutdown, cancelAudioImportShutdown } from "@/lib/audioImportWriteFence";
import { getSavedCustomCursorPref, saveCustomCursorPref } from "@/hooks/videoStatePreferences";
import type { RecordingMode } from "@/types/video";
import { notifyPreviewMediaMounted } from "@/lib/previewMediaLifecycle";

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
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    localStorage.clear();
  });

  afterEach(async () => {
    recordingHandoff.cancelPrepare();
    await recordingHandoff.stop(async () => true);
  });

  it.each([
    [undefined, "withoutCursor", true],
    [false, "withoutCursor", false],
    [true, "withoutCursor", true],
    [true, "withCursor", false],
    [false, "withCursor", false],
  ] as const)("initializes recording with saved pointer %s in mode %s", async (saved, mode, expected) => {
    if (saved !== undefined) saveCustomCursorPref(saved);
    const props = makeProps();
    props.currentVideo = null;
    props.videoRef = { current: null };
    props.canvasRef = { current: null };
    props.videoControllerRef.current = {
      loadVideo: vi.fn().mockResolvedValue("blob:new-recording"),
    } as unknown as NonNullable<UseRecordingProps["videoControllerRef"]["current"]>;
    vi.mocked(invoke).mockImplementation(async (command) => command === "stop_recording" ? {
      videoUrl: "blob:new-recording",
      mouseData: [],
      inputEvents: [],
    } : undefined);
    const { result } = renderHook(() => useRecording(props));
    await act(async () => result.current.startNewRecording("0", mode as RecordingMode));
    let stopped: Awaited<ReturnType<typeof result.current.handleStopRecording>> = null;
    await act(async () => { stopped = await result.current.handleStopRecording(); });
    expect(result.current.error).toBeNull();
    expect(stopped).toMatchObject({ initialSegment: { useCustomCursor: expected } });
    expect(getSavedCustomCursorPref()).toBe(saved ?? true);
  });

  it("coalesces concurrent recording starts into one native request", async () => {
    const start = deferred<unknown>();
    vi.mocked(invoke).mockImplementation((command) => command === "start_recording" ? start.promise : Promise.resolve(null));
    const { result } = renderHook(() => useRecording(makeProps()));

    let first!: Promise<void>;
    let second!: Promise<void>;
    act(() => {
      first = result.current.startNewRecording("0", "withoutCursor");
      second = result.current.startNewRecording("0", "withoutCursor");
    });

    expect(vi.mocked(invoke).mock.calls.filter(([command]) => command === "start_recording")).toHaveLength(1);
    expect(invoke).toHaveBeenCalledWith("start_recording", expect.any(Object));
    start.resolve(undefined);
    await act(async () => Promise.all([first, second]));
    expect(result.current.isRecording).toBe(true);
  });

  it("rejects a new recording while shutdown owns admission", async () => {
    vi.mocked(invoke).mockResolvedValue(null);
    const token = beginAudioImportShutdown();
    expect(token).not.toBeNull();
    const { result } = renderHook(() => useRecording(makeProps()));
    await act(async () => result.current.startNewRecording("0", "withoutCursor"));
    expect(invoke).not.toHaveBeenCalledWith("start_recording", expect.anything());
    expect(result.current.error).toContain("closing");
    cancelAudioImportShutdown(token);
  });

  it("restores a still-finalizing recording after the editor reloads", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_pending_recording") return { recordingActive: true, recordingMode: "withCursor" };
      if (command === "stop_recording") throw new Error("still finishing");
      return null;
    });
    const retry = vi.fn();
    window.addEventListener("sr-recording-retry-required", retry);
    const { result } = renderHook(() => useRecording(makeProps()));
    await waitFor(() => expect(retry).toHaveBeenCalledOnce());
    await act(async () => { await result.current.startNewRecording("0", "withoutCursor"); });
    expect(invoke).not.toHaveBeenCalledWith("start_recording", expect.anything());
    await act(async () => { await result.current.handleStopRecording(); });
    expect(invoke).toHaveBeenCalledWith("stop_recording");
    window.removeEventListener("sr-recording-retry-required", retry);
  });

  it("waits for its deferred preview controller before recovering a finalized recording", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => command === "get_pending_recording" ? {
      recordingId: "a".repeat(32), recordingMode: "withoutCursor", videoFilePath: "recording.mp4", videoUrl: "blob:recording",
    } : null);
    const props = makeProps();
    const recover = vi.fn();
    window.addEventListener("sr-recover-recording", recover);
    const view = renderHook(() => useRecording(props));
    try {
      await act(async () => {});
      expect(recover).not.toHaveBeenCalled();
      props.videoControllerRef.current = {} as NonNullable<UseRecordingProps["videoControllerRef"]["current"]>;
      notifyPreviewMediaMounted(document.createElement("video"), document.createElement("canvas"));
      expect(recover).not.toHaveBeenCalled();
      notifyPreviewMediaMounted(props.videoRef.current, props.canvasRef.current);
      expect(recover).toHaveBeenCalledOnce();
      notifyPreviewMediaMounted(props.videoRef.current, props.canvasRef.current);
      expect(recover).toHaveBeenCalledOnce();
    } finally {
      view.unmount();
      window.removeEventListener("sr-recover-recording", recover);
    }
  });
});
