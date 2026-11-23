import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useTimelineSegmentActions } from "@/components/timeline/useTimelineSegmentActions";
import { getVisibleSubtitleSegments } from "@/lib/subtitleTracks";
import type { VideoSegment } from "@/types/video";

function renderActions(segment: VideoSegment) {
  const setSegment = vi.fn();
  const beginBatch = vi.fn();
  const commitBatch = vi.fn();
  const { result } = renderHook(() => useTimelineSegmentActions({
    duration: 10, segment, setSegment, setCurrentTime: vi.fn(),
    videoRef: { current: null }, beginBatch, commitBatch,
  }));
  return { result, setSegment, beginBatch, commitBatch };
}

describe("timeline segment actions delete everything in a lane", () => {
  it("leaves an empty text lane in one history step", () => {
    const segment = {
      trimStart: 0, trimEnd: 10, zoomKeyframes: [],
      textSegments: [
        { id: "a", startTime: 0, endTime: 2, text: "A" },
        { id: "b", startTime: 3, endTime: 5, text: "B" },
      ],
    } as unknown as VideoSegment;
    const { result, setSegment, beginBatch, commitBatch } = renderActions(segment);
    act(() => result.current.handleDeleteTextSegments(["a", "b"]));
    expect(setSegment).toHaveBeenCalledWith(expect.objectContaining({ textSegments: [] }));
    expect(beginBatch).toHaveBeenCalledOnce();
    expect(commitBatch).toHaveBeenCalledOnce();
  });

  it("leaves no visible subtitles after deleting every subtitle", () => {
    const segment = {
      trimStart: 0, trimEnd: 10, zoomKeyframes: [], textSegments: [],
      subtitleSegments: [
        { id: "s1", startTime: 0, endTime: 2, text: "one", style: {} },
        { id: "s2", startTime: 2, endTime: 4, text: "two", style: {} },
      ],
    } as unknown as VideoSegment;
    const { result, setSegment } = renderActions(segment);
    act(() => result.current.handleDeleteSubtitleSegments(["s1", "s2"]));
    const next = setSegment.mock.calls[0]?.[0] as VideoSegment;
    expect(getVisibleSubtitleSegments(next)).toEqual([]);
  });
});
