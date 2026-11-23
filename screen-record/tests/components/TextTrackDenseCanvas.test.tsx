import { fireEvent, render } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { TextTrack } from "@/components/timeline/TextTrack";
import type { TextSegment, VideoSegment } from "@/types/video";

function texts(count: number): TextSegment[] {
  return Array.from({ length: count }, (_, index) => ({
    id: `t${index}`,
    startTime: index * 0.1,
    endTime: index * 0.1 + 0.05,
    text: `Text ${index}`,
    style: { fontVariations: {}, color: "#ffffff", fontSize: 24, x: 50, y: 50, textAlign: "center", opacity: 1 },
  })) as TextSegment[];
}

function segmentWith(count: number): VideoSegment {
  return { trimStart: 0, trimEnd: 100, zoomKeyframes: [], textSegments: texts(count) } as VideoSegment;
}

describe("TextTrack dense mode", () => {
  it("paints non-DOM texts on a canvas and selects them by hit-testing", () => {
    const onTextClick = vi.fn();
    const onEmptyClick = vi.fn();
    const { container } = render(
      <TextTrack
        segment={segmentWith(300)}
        duration={100}
        editingTextId="t5"
        onTextClick={onTextClick}
        onHandleDragStart={vi.fn()}
        onEmptyClick={onEmptyClick}
        canvasWidthPx={1000}
        visibleTimeRange={{ startTime: 0, endTime: 30 }}
      />,
    );
    expect(container.querySelector(".segment-blocks-canvas")).not.toBeNull();
    expect(container.querySelectorAll(".text-segment").length).toBeLessThan(300);

    const track = container.querySelector<HTMLElement>(".text-track")!;
    track.getBoundingClientRect = () =>
      ({ x: 0, y: 0, left: 0, top: 0, width: 1000, height: 28, right: 1000, bottom: 28, toJSON() {} }) as DOMRect;
    // 12.02 s lies inside t120 (12.0 - 12.05), which has no DOM block in dense mode.
    expect([...container.querySelectorAll(".text-segment")].some((el) => el.textContent === "Text 120")).toBe(false);
    fireEvent.pointerDown(track, { clientX: 120.2, clientY: 10, button: 0 });
    expect(onTextClick).toHaveBeenCalledWith("t120");
    expect(onEmptyClick).not.toHaveBeenCalled();

    // A gap between texts still seeks.
    fireEvent.pointerDown(track, { clientX: 120.8, clientY: 10, button: 0 });
    expect(onEmptyClick).toHaveBeenCalledOnce();
  });

  it("keeps a DOM-only track below the dense threshold", () => {
    const { container } = render(
      <TextTrack segment={segmentWith(10)} duration={10} editingTextId={null} onTextClick={vi.fn()} onHandleDragStart={vi.fn()} />,
    );
    expect(container.querySelector(".segment-blocks-canvas")).toBeNull();
    expect(container.querySelectorAll(".text-segment")).toHaveLength(10);
  });
});
