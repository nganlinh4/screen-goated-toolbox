import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useAutoZoom } from "@/hooks/useAutoZoom";
import type { MousePosition, VideoSegment } from "@/types/video";

const manager = vi.hoisted(() => ({ updateProject: vi.fn(async () => true) }));
vi.mock("@/lib/projectManager", () => ({ projectManager: manager }));
vi.mock("@/lib/autoZoom", () => ({
  autoZoomGenerator: { generateMotionPath: vi.fn(() => [{ time: 0, x: 1, y: 1, zoom: 2 }]) },
}));

function setup(segment: VideoSegment) {
  const setSegment = vi.fn();
  const setActivePanel = vi.fn();
  const video = Object.assign(document.createElement("video"), {});
  Object.defineProperties(video, { videoWidth: { value: 1920 }, videoHeight: { value: 1080 } });
  const hook = renderHook(() => useAutoZoom({
    segment, setSegment, videoRef: { current: video },
    mousePositions: [{ x: 1, y: 1, timestamp: 0 }] as MousePosition[],
    duration: 10, setActivePanel,
  }));
  return { ...hook, setSegment, setActivePanel };
}

describe("useAutoZoom", () => {
  afterEach(() => { vi.useRealTimers(); });

  it("applies a generated path to the editor segment without writing the project root", async () => {
    vi.useFakeTimers();
    const { result, setSegment, setActivePanel } = setup({ trimStart: 0, trimEnd: 10 } as VideoSegment);
    act(() => result.current.handleAutoZoom());
    await act(async () => { await vi.advanceTimersByTimeAsync(1); });
    expect(setSegment).toHaveBeenCalledWith(expect.objectContaining({
      smoothMotionPath: [{ time: 0, x: 1, y: 1, zoom: 2 }],
    }));
    expect(setActivePanel).toHaveBeenCalledWith("zoom");
    expect(manager.updateProject).not.toHaveBeenCalled();
  });

  it("clears an active path without writing the project root", () => {
    const { result, setSegment } = setup({
      trimStart: 0, trimEnd: 10, smoothMotionPath: [{ time: 0, x: 1, y: 1, zoom: 2 }],
    } as VideoSegment);
    act(() => result.current.handleAutoZoom());
    expect(setSegment).toHaveBeenCalledWith(expect.objectContaining({ smoothMotionPath: [] }));
    expect(manager.updateProject).not.toHaveBeenCalled();
  });
});
