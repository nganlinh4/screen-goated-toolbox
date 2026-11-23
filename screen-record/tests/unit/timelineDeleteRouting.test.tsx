import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useTrackRangeSelect } from "@/components/timeline/useTrackRangeSelect";
import { useAdjustableLineTrack } from "@/components/timeline/useAdjustableLineTrack";

type Point = { time: number; value: number };

function pressDelete(target: EventTarget = document.body) {
  target.dispatchEvent(new KeyboardEvent("keydown", { key: "Delete", code: "Delete", bubbles: true, cancelable: true }));
}

function renderLine(onUpdatePoints: (points: Point[]) => void) {
  return renderHook(() => useAdjustableLineTrack<Point>({
    points: [{ time: 0, value: 1 }, { time: 1, value: 0.5 }, { time: 2, value: 1 }],
    duration: 2, onUpdatePoints, getValue: (p) => p.value,
    createPoint: (time, value) => ({ time, value }),
  } as unknown as Parameters<typeof useAdjustableLineTrack<Point>>[0]));
}

describe("timeline Delete routing", () => {
  afterEach(() => { document.body.innerHTML = ""; });

  it("lets a hovered curve point outrank a stale selection in another lane", () => {
    const onDeleteSelected = vi.fn();
    const range = renderHook(() => useTrackRangeSelect(
      [{ id: "sub-1", startTime: 0, endTime: 1 }], 10, undefined, undefined, onDeleteSelected,
    ));
    act(() => range.result.current.addSegmentSelection("sub-1"));
    const onUpdatePoints = vi.fn();
    const line = renderLine(onUpdatePoints);
    act(() => line.result.current.setHoveredIdx(1));

    act(() => pressDelete());
    expect(onDeleteSelected).not.toHaveBeenCalled();
    expect(onUpdatePoints).toHaveBeenCalledWith([{ time: 0, value: 1 }, { time: 2, value: 1 }]);

    act(() => pressDelete());
    expect(onDeleteSelected).toHaveBeenCalledWith(["sub-1"]);
  });

  it("never deletes a hovered point while the user types in a text field", () => {
    const onUpdatePoints = vi.fn();
    const line = renderLine(onUpdatePoints);
    act(() => line.result.current.setHoveredIdx(1));
    const editor = document.body.appendChild(document.createElement("textarea"));
    act(() => pressDelete(editor));
    expect(onUpdatePoints).not.toHaveBeenCalled();
  });
});
