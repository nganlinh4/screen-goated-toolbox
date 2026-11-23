import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useTimelineSegmentActions } from "@/components/timeline/useTimelineSegmentActions";
import type { VideoSegment } from "@/types/video";

describe("timeline visibility deletion", () => {
  it.each([
    ["pointer", "cursorVisibilitySegments"],
    ["keyboard", "keyboardVisibilitySegments"],
    ["keyboardMouse", "keyboardMouseVisibilitySegments"],
    ["webcam", "webcamVisibilitySegments"],
  ] as const)("keeps the %s lane hidden after deleting its final segment", (kind, field) => {
    const segment: VideoSegment = {
      trimStart: 0,
      trimEnd: 10,
      zoomKeyframes: [],
      textSegments: [],
      keystrokeMode: kind === "keyboard" || kind === "keyboardMouse" ? kind : "off",
      [field]: [{ id: "visible", startTime: 0, endTime: 10 }],
    };
    const setSegment = vi.fn();
    const beginBatch = vi.fn();
    const commitBatch = vi.fn();
    const { result } = renderHook(() => useTimelineSegmentActions({
      duration: 10,
      segment,
      setSegment,
      setCurrentTime: vi.fn(),
      videoRef: { current: null },
      beginBatch,
      commitBatch,
    }));
    act(() => {
      if (kind === "pointer") result.current.handleDeletePointerSegments(["visible"]);
      else if (kind === "webcam") result.current.handleDeleteWebcamSegments(["visible"]);
      else result.current.handleDeleteKeystrokeSegments(["visible"]);
    });
    expect(setSegment).toHaveBeenCalledWith(expect.objectContaining({ [field]: [] }));
    expect(beginBatch).toHaveBeenCalledOnce();
    expect(commitBatch).toHaveBeenCalledOnce();
  });
});
