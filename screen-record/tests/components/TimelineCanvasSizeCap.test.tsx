import { act, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { SegmentBlocksCanvas } from "@/components/timeline/SegmentBlocksCanvas";
import {
  getTimelineCanvasSize,
  MAX_CANVAS_SIDE_PX,
} from "@/components/timeline/timelineCanvasSize";

describe("timeline canvas size cap", () => {
  const originalResizeObserver = window.ResizeObserver;
  afterEach(() => {
    window.ResizeObserver = originalResizeObserver;
    vi.unstubAllGlobals();
  });

  it("caps the backing store and scales drawing to the capped width", () => {
    const size = getTimelineCanvasSize(20_000, 40, 2);
    expect(size.width).toBe(MAX_CANVAS_SIDE_PX);
    expect(size.height).toBe(80);
    expect(size.scaleX).toBeCloseTo(MAX_CANVAS_SIDE_PX / 20_000);
    expect(size.scaleY).toBe(2);
    expect(getTimelineCanvasSize(1_000, 40, 2)).toEqual({ width: 2_000, height: 80, scaleX: 2, scaleY: 2 });
  });

  it("never sizes a max-zoom segment canvas past the browser limit at dpr 2", () => {
    let notify: ResizeObserverCallback | null = null;
    vi.stubGlobal("devicePixelRatio", 2);
    // The setup polyfill is writable but not configurable, so assign instead of stubbing.
    window.ResizeObserver = class {
      constructor(callback: ResizeObserverCallback) {
        notify = callback;
      }
      observe() {}
      unobserve() {}
      disconnect() {}
    } as unknown as typeof ResizeObserver;
    const { container } = render(
      <SegmentBlocksCanvas
        segments={[]}
        duration={10}
        visibleRange={null}
        colorVar="--x"
        fallbackColor="#fff"
      />,
    );
    const canvas = container.querySelector("canvas") as HTMLCanvasElement;
    act(() => {
      notify?.(
        [{ contentRect: { width: 1_700 * 12, height: 24 } } as ResizeObserverEntry],
        {} as ResizeObserver,
      );
    });
    expect(canvas.width).toBe(MAX_CANVAS_SIDE_PX);
    expect(canvas.height).toBe(48);
  });
});
