import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  isTimelinePointerDragActive,
  startTimelinePointerDrag,
} from "@/components/timeline/timelinePointerDrag";
import { useZoomBlockRangeEditing } from "@/components/timeline/useZoomBlockRangeEditing";
import type { ZoomBlock } from "@/types/video";

// jsdom has no PointerEvent constructor; a MouseEvent with the pointer type name
// reaches the same window listeners with clientX.
function pointer(type: string, clientX = 0) {
  window.dispatchEvent(new MouseEvent(type, { clientX }));
}

function trackElement(rects: Array<{ left: number; width: number }>) {
  const element = document.createElement("div");
  let call = 0;
  element.getBoundingClientRect = () => {
    const rect = rects[Math.min(call, rects.length - 1)];
    call += 1;
    return { left: rect.left, width: rect.width, top: 0, height: 28, right: rect.left + rect.width, bottom: 28, x: rect.left, y: 0, toJSON: () => ({}) } as DOMRect;
  };
  return element;
}

describe("shared timeline pointer drag", () => {
  afterEach(() => vi.restoreAllMocks());

  it("reads the rect on every move, pauses follow-playhead while active, and ends on cancel", () => {
    const element = trackElement([{ left: 0, width: 100 }, { left: -50, width: 100 }]);
    const lefts: number[] = [];
    const onEnd = vi.fn();
    startTimelinePointerDrag({ element, onMove: (_event, rect) => lefts.push(rect.left), onEnd });

    expect(isTimelinePointerDragActive()).toBe(true);
    pointer("pointermove", 10);
    pointer("pointermove", 10);
    expect(lefts).toEqual([0, -50]);

    pointer("pointercancel");
    expect(onEnd).toHaveBeenCalledWith(true);
    expect(isTimelinePointerDragActive()).toBe(false);
    pointer("pointermove", 20);
    expect(lefts).toHaveLength(2);
  });

  it("keeps a zoom-block edge under the pointer while follow-playhead scrolls the track", () => {
    const blocks = [{ id: "z", startTime: 1, endTime: 4, easeIn: 0.5, easeOut: 0.5 }] as unknown as ZoomBlock[];
    const onUpdateBlocks = vi.fn();
    const commitBatch = vi.fn();
    const { result } = renderHook(() =>
      useZoomBlockRangeEditing({ duration: 10, blocks, onUpdateBlocks, beginBatch: vi.fn(), commitBatch }),
    );
    // The 1000px track scrolls 200px left between the pointer-down and the move.
    const element = trackElement([{ left: -200, width: 1000 }]);

    act(() => result.current.startResizeBlock(0, "end", element));
    act(() => pointer("pointermove", 400));
    act(() => pointer("pointerup", 400));

    expect(commitBatch).toHaveBeenCalledTimes(1);
    const committed = onUpdateBlocks.mock.calls.at(-1)?.[0] as ZoomBlock[];
    // clientX 400 over a track whose left is -200 is 600px in, i.e. 6s of 10s.
    expect(committed[0].endTime).toBeCloseTo(6);
    expect(isTimelinePointerDragActive()).toBe(false);
  });
});
