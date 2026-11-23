import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useVisibilitySegmentDrag } from "@/components/timeline/useVisibilitySegmentDrag";
import type { CursorVisibilitySegment, VideoSegment } from "@/types/video";

const visibility: CursorVisibilitySegment[] = [{ id: "v1", startTime: 0, endTime: 10 }];
const segment = { trimStart: 0, trimEnd: 10, cursorVisibilitySegments: visibility } as unknown as VideoSegment;

function renderDrag() {
  const onSplit = vi.fn();
  const { result } = renderHook(() =>
    useVisibilitySegmentDrag({
      duration: 10,
      segment,
      setSegment: vi.fn(),
      getTimeFromClientX: (clientX) => clientX / 10,
      beginBatch: vi.fn(),
      commitBatch: vi.fn(),
      snapshotOriginals: () => visibility,
      onDragStart: vi.fn(),
      writeDuringDrag: (current) => current,
      getSegmentsForSplit: () => visibility,
      setSegmentsAfterSplit: (current) => current,
      onSplit,
    }),
  );
  return { result, onSplit };
}

describe("visibility segment split clicks after a drag", () => {
  afterEach(() => vi.restoreAllMocks());

  it("ignores the click that ends a drag but accepts the next deliberate click", () => {
    let now = 1_000;
    vi.spyOn(performance, "now").mockImplementation(() => now);
    const { result, onSplit } = renderDrag();

    act(() => result.current.handleDragStart("v1", "end"));
    act(() => result.current.handleDrag(80));
    act(() => result.current.reset());

    act(() => result.current.handleClick("v1", 5));
    expect(onSplit).not.toHaveBeenCalled();

    // Released off the segment: no click followed. A later click must split.
    act(() => result.current.handleDragStart("v1", "end"));
    act(() => result.current.handleDrag(90));
    act(() => result.current.reset());
    now += 1_000;
    act(() => result.current.handleClick("v1", 5));
    expect(onSplit).toHaveBeenCalledTimes(1);
  });
});
