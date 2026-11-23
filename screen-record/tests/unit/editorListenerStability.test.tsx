import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useMediaPipelineRecovery } from "@/hooks/useMediaPipelineRecovery";
import { useKeystrokeDrag, type UseKeystrokeDragParams } from "@/hooks/useKeystrokeDrag";
import { usePreviewInteraction } from "@/hooks/usePreviewInteraction";
import { useAppShortcuts, type UseAppShortcutsParams } from "@/hooks/useAppShortcuts";
import { useHotkeys } from "@/hooks/useAppHooks";
import type { VideoController } from "@/lib/videoController";
import type { Project, VideoSegment } from "@/types/video";

vi.mock("@/lib/mediaServer", () => ({ getMediaServerUrl: vi.fn(async () => "http://media/p.mp4") }));
// The real renderer module paints canvas backgrounds at import, which jsdom cannot run.
vi.mock("@/lib/videoRenderer", () => ({
  videoRenderer: {
    getLastCalculatedState: vi.fn(() => ({ positionX: 0.5, positionY: 0.5, zoomFactor: 2 })),
    getKeystrokeOverlayEditBounds: vi.fn(() => null),
    getKeystrokeOverlayConfig: vi.fn(() => ({ x: 50, y: 90, scale: 1 })),
    handleMouseDown: vi.fn(() => null),
    handleMouseMove: vi.fn(),
    handleMouseUp: vi.fn(),
  },
}));

function countAdds(spy: ReturnType<typeof vi.spyOn>, type: string) {
  return spy.mock.calls.filter(([eventType]) => eventType === type).length;
}

afterEach(() => {
  delete (window as Window & { invoke?: unknown }).invoke;
});

describe("media pipeline recovery listener", () => {
  it("subscribes once across playback frames and resumes from the latest time", async () => {
    const addSpy = vi.spyOn(window, "addEventListener");
    const loadVideo = vi.fn(async () => "blob:next");
    const controller = { videoLoadVersion: 1, loadVideo, play: vi.fn() };
    const base = {
      currentProjectDataRef: { current: { id: "p" } as Project },
      currentRawVideoPath: "p.mp4", currentRawMicAudioPath: "", currentRawWebcamVideoPath: "",
      isPlaying: false, projectId: "p", seek: vi.fn(),
      segmentDeviceAudioAvailable: false, setCurrentAudio: vi.fn(), setCurrentMicAudio: vi.fn(),
      setCurrentVideo: vi.fn(), setCurrentWebcamVideo: vi.fn(), setPreviewAudioResetKey: vi.fn(),
      videoControllerRef: { current: controller as unknown as VideoController },
    };
    const { rerender, unmount } = renderHook((props: { currentTime: number }) =>
      useMediaPipelineRecovery({ ...base, currentTime: props.currentTime }), { initialProps: { currentTime: 0 } });
    for (let frame = 1; frame <= 30; frame += 1) rerender({ currentTime: frame / 60 });
    expect(countAdds(addSpy, "sr-reset-media-pipeline")).toBe(1);

    rerender({ currentTime: 7.25 });
    await act(async () => {
      window.dispatchEvent(new CustomEvent("sr-reset-media-pipeline"));
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(loadVideo).toHaveBeenCalledWith(expect.objectContaining({ initialTime: 7.25 }));
    unmount();
  });
});

describe("keystroke drag listeners", () => {
  it("binds window listeners once while time and segment change", () => {
    const canvasRef = { current: document.createElement("canvas") };
    const addSpy = vi.spyOn(window, "addEventListener");
    const segment = { trimStart: 0, trimEnd: 10 } as VideoSegment;
    const params = (currentTime: number, seg: VideoSegment): UseKeystrokeDragParams => ({
      segment: seg, setSegment: vi.fn(), canvasRef, segmentRef: { current: seg },
      isDraggingKeystrokeOverlayRef: { current: false }, isResizingKeystrokeOverlayRef: { current: false },
      keystrokeOverlayDragStartRef: { current: null }, currentTime,
      getKeystrokeTimelineDuration: () => 10, setIsPreviewDragging: vi.fn(),
      setIsKeystrokeResizeDragging: vi.fn(), setIsKeystrokeResizeHandleHover: vi.fn(),
      setIsKeystrokeOverlaySelected: vi.fn(), setEditingTextId: vi.fn(), setEditingSubtitleId: vi.fn(),
      setActivePanel: vi.fn(), handleOverlayDragMove: vi.fn(), selectedTextIdsRef: { current: [] },
      selectedSubtitleIdsRef: { current: [] }, beginBatch: vi.fn(), commitBatch: vi.fn(),
    });
    const { rerender, unmount } = renderHook((props: { t: number; seg: VideoSegment }) =>
      useKeystrokeDrag(params(props.t, props.seg)), { initialProps: { t: 0, seg: segment } });
    for (let frame = 1; frame <= 30; frame += 1) rerender({ t: frame / 60, seg: { ...segment } });
    expect(countAdds(addSpy, "mousemove")).toBe(1);
    expect(countAdds(addSpy, "mouseup")).toBe(1);
    unmount();
  });
});

describe("preview interaction", () => {
  function previewParams(overrides: Record<string, unknown> = {}) {
    const container = document.createElement("div");
    return {
      currentVideo: "video.mp4", isCropping: false, activePanel: "zoom" as const, setActivePanel: vi.fn(),
      isPlaying: false, handleTogglePlayPause: vi.fn(), handleAddKeyframe: vi.fn(),
      beginBatch: vi.fn(), commitBatch: vi.fn(), previewContainerRef: { current: container },
      isKeystrokeResizeDragging: false, isKeystrokeResizeHandleHover: false,
      segment: null, currentTime: 0, ...overrides,
    };
  }

  it("commits an open drag batch when the preview unmounts mid-drag", () => {
    const p = previewParams();
    const { result, unmount } = renderHook(() => usePreviewInteraction(p));
    const target = document.createElement("div");
    act(() => {
      result.current.handlePreviewMouseDown({
        clientX: 10, clientY: 10, preventDefault() {}, stopPropagation() {}, currentTarget: target,
      } as unknown as React.MouseEvent);
    });
    expect(p.beginBatch).toHaveBeenCalledOnce();
    expect(p.commitBatch).not.toHaveBeenCalled();
    unmount();
    expect(p.commitBatch).toHaveBeenCalledOnce();
  });

  it("commits an open wheel batch on unmount and keeps one wheel listener during playback", () => {
    const p = previewParams();
    const container = p.previewContainerRef.current;
    const addSpy = vi.spyOn(container, "addEventListener");
    const { rerender, unmount } = renderHook((props: { t: number }) =>
      usePreviewInteraction({ ...p, currentTime: props.t }), { initialProps: { t: 0 } });
    for (let frame = 1; frame <= 30; frame += 1) rerender({ t: frame / 60 });
    expect(countAdds(addSpy, "wheel")).toBe(1);

    act(() => {
      container.dispatchEvent(new WheelEvent("wheel", { deltaY: -100, cancelable: true }));
    });
    expect(p.beginBatch).toHaveBeenCalledOnce();
    unmount();
    expect(p.commitBatch).toHaveBeenCalledOnce();
  });
});

describe("hotkey capture", () => {
  it("keeps editor shortcuts from acting while the hotkey dialog captures keys", () => {
    (window as Window & { invoke?: unknown }).invoke = vi.fn(async (cmd: string) => (cmd === "get_hotkeys" ? [] : null));
    const handleDeleteText = vi.fn();
    const togglePlayPause = vi.fn();
    const { result } = renderHook(() => {
      const hotkeys = useHotkeys();
      const shortcuts: UseAppShortcutsParams = {
        togglePlayPause, currentTime: 10, duration: 60, seek: vi.fn(), isCropping: false,
        // Same signal the app passes from useAppLateEffects.
        isModalOpen: hotkeys.showHotkeyDialog,
        editingKeyframeId: null, editingTextId: "text-1", editingSubtitleId: null,
        editingKeystrokeSegmentId: null, editingPointerId: null, segment: null,
        setSegment: vi.fn(), setEditingKeyframeId: vi.fn(), handleDeleteText,
        handleDeleteSubtitle: vi.fn(), handleDeleteKeystrokeSegment: vi.fn(),
        handleDeletePointerSegment: vi.fn(), canUndo: true, canRedo: true, undo: vi.fn(), redo: vi.fn(),
        setSeekIndicatorKey: vi.fn(), setSeekIndicatorDir: vi.fn(),
      };
      useAppShortcuts(shortcuts);
      return hotkeys;
    });
    act(() => result.current.openHotkeyDialog());
    act(() => {
      document.body.dispatchEvent(new KeyboardEvent("keydown", { code: "Delete", key: "Delete", bubbles: true, cancelable: true }));
      document.body.dispatchEvent(new KeyboardEvent("keydown", { code: "Space", key: " ", bubbles: true, cancelable: true }));
    });
    expect(handleDeleteText).not.toHaveBeenCalled();
    expect(togglePlayPause).not.toHaveBeenCalled();
  });
});
