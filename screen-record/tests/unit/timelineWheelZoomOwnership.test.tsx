import { act, fireEvent, render, screen } from "@testing-library/react";
import { Suspense, useRef } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useTimelineViewport } from "@/components/timeline/useTimelineViewport";
import type { VideoSegment } from "@/types/video";

let transitionBlock: { promise: Promise<void>; observed: number } | null = null;

const segment: VideoSegment = {
  trimStart: 0,
  trimEnd: 60,
  zoomKeyframes: [],
  textSegments: [],
};

function Harness({ time, active = true }: { time: number; active?: boolean }) {
  const timelineRef = useRef<HTMLDivElement>(null);
  const videoRef = useRef<HTMLVideoElement>(null);
  const viewport = useTimelineViewport({
    duration: active ? 60 : 0,
    currentTime: time,
    segment: active ? segment : null,
    timelineRef,
    videoRef,
    isPlaying: false,
    isInteracting: false,
  });
  if (transitionBlock && viewport.visibleTimeRange &&
      viewport.visibleTimeRange.startTime < 20 && viewport.visibleTimeRange.endTime < 55) {
    transitionBlock.observed += 1;
    throw transitionBlock.promise;
  }
  return (
    <>
      <div ref={viewport.viewportRef} data-testid="viewport">
        <div data-testid="canvas" style={{ width: viewport.canvasWidth }}>
          <div ref={timelineRef} />
        </div>
      </div>
      <div ref={viewport.scrollbarTrackRef} data-testid="track" onPointerDown={viewport.handleScrollbarTrackPointerDown}>
        <div ref={viewport.scrollbarThumbRef} data-testid="thumb" onPointerDown={viewport.handleScrollbarThumbPointerDown} />
      </div>
      <output data-testid="zoom">{viewport.zoom}</output>
      <output data-testid="range">{JSON.stringify(viewport.visibleTimeRange)}</output>
    </>
  );
}

describe("timeline wheel zoom ownership", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) =>
      window.setTimeout(() => callback(performance.now()), 16));
    vi.stubGlobal("cancelAnimationFrame", (id: number) => window.clearTimeout(id));
    vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(1032);
  });

  afterEach(() => {
    transitionBlock = null;
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
    vi.useRealTimers();
  });

  it("keeps wheel zoom across unrelated playback renders and resumes visible-range updates", () => {
    const view = render(<Harness time={0} />);
    const viewport = screen.getByTestId("viewport");
    fireEvent.wheel(viewport, { deltaY: -1500, clientX: 500 });
    expect(screen.getByTestId("canvas").style.width).toBe("400%");
    view.rerender(<Harness time={1} />);
    act(() => vi.advanceTimersByTime(150));
    expect(Number(screen.getByTestId("zoom").textContent)).toBeCloseTo(4);
    expect(screen.getByTestId("canvas").style.width).toBe("400%");
    const beforeScroll = screen.getByTestId("range").textContent;
    viewport.scrollLeft = 2500;
    fireEvent.scroll(viewport);
    act(() => vi.advanceTimersByTime(20));
    expect(screen.getByTestId("range").textContent).not.toBe(beforeScroll);
  });

  it("accumulates rapid wheel events even when each is followed by a render", () => {
    const view = render(<Harness time={0} />);
    const viewport = screen.getByTestId("viewport");
    fireEvent.wheel(viewport, { deltaY: -200, clientX: 500 });
    view.rerender(<Harness time={1} />);
    fireEvent.wheel(viewport, { deltaY: -200, clientX: 500 });
    view.rerender(<Harness time={2} />);
    act(() => vi.advanceTimersByTime(150));
    expect(Number(screen.getByTestId("zoom").textContent)).toBeCloseTo(1.96);
    expect(parseFloat(screen.getByTestId("canvas").style.width)).toBeCloseTo(196);
  });

  it("releases the anchor when a wheel reversal returns to the committed zoom", () => {
    render(<Harness time={0} />);
    const viewport = screen.getByTestId("viewport");
    fireEvent.wheel(viewport, { deltaY: -1500, clientX: 500 });
    act(() => vi.advanceTimersByTime(150));
    expect(Number(screen.getByTestId("zoom").textContent)).toBeCloseTo(4);
    fireEvent.wheel(viewport, { deltaY: -125, clientX: 500 });
    fireEvent.wheel(viewport, { deltaY: 100, clientX: 500 });
    act(() => vi.advanceTimersByTime(150));
    expect(Number(screen.getByTestId("zoom").textContent)).toBeCloseTo(4);
    const beforeScroll = screen.getByTestId("range").textContent;
    viewport.scrollLeft = 2500;
    fireEvent.scroll(viewport);
    act(() => vi.advanceTimersByTime(20));
    expect(screen.getByTestId("range").textContent).not.toBe(beforeScroll);
  });

  it("does not let a suspended no-op zoom transition overwrite a newer scroll range", async () => {
    render(<Suspense fallback={<span>Loading</span>}><Harness time={0} /></Suspense>);
    const viewport = screen.getByTestId("viewport");
    fireEvent.wheel(viewport, { deltaY: -1500, clientX: 500 });
    act(() => vi.advanceTimersByTime(150));

    fireEvent.wheel(viewport, { deltaY: -125, clientX: 500 });
    fireEvent.wheel(viewport, { deltaY: 100, clientX: 500 });
    act(() => vi.advanceTimersByTime(20));
    let release!: () => void;
    const promise = new Promise<void>((resolve) => { release = resolve; });
    transitionBlock = { promise, observed: 0 };
    act(() => vi.advanceTimersByTime(100));
    expect(transitionBlock.observed).toBeGreaterThan(0);

    viewport.scrollLeft = 2500;
    fireEvent.scroll(viewport);
    act(() => vi.advanceTimersByTime(20));
    const afterScroll = screen.getByTestId("range").textContent;
    expect(JSON.parse(afterScroll!).startTime).toBeGreaterThan(0);
    transitionBlock = null;
    await act(async () => { release(); await promise; });
    expect(screen.getByTestId("range").textContent).toBe(afterScroll);
    expect(Number(screen.getByTestId("zoom").textContent)).toBeCloseTo(4);
  });

  it("cancels a pending wheel update when the active segment is removed", () => {
    const view = render(<Harness time={0} />);
    const viewport = screen.getByTestId("viewport");
    fireEvent.wheel(viewport, { deltaY: -200, clientX: 500 });
    view.rerender(<Harness time={0} active={false} />);
    act(() => vi.advanceTimersByTime(150));
    expect(Number(screen.getByTestId("zoom").textContent)).toBe(1);
    expect(screen.getByTestId("canvas").style.width).toBe("100%");
    view.rerender(<Harness time={1} />);
    fireEvent.wheel(viewport, { deltaY: -200, clientX: 500 });
    act(() => vi.advanceTimersByTime(150));
    expect(Number(screen.getByTestId("zoom").textContent)).toBeCloseTo(1.4);
  });

  it("preserves a native scroll made before the wheel's programmatic frame clears", () => {
    render(<Harness time={0} />);
    const viewport = screen.getByTestId("viewport");
    fireEvent.wheel(viewport, { deltaY: -1500, clientX: 500 });
    viewport.scrollLeft = 2500;
    fireEvent.scroll(viewport);
    act(() => vi.advanceTimersByTime(150));
    expect(Number(screen.getByTestId("zoom").textContent)).toBeCloseTo(4);
    expect(viewport.scrollLeft).toBeCloseTo(2500);
    const range = JSON.parse(screen.getByTestId("range").textContent!);
    expect(range.startTime).toBeGreaterThan(0);
  });

  it("preserves a scrollbar drag made while wheel zoom is pending", () => {
    render(<Harness time={0} />);
    const viewport = screen.getByTestId("viewport");
    fireEvent.wheel(viewport, { deltaY: -1500, clientX: 500 });
    act(() => vi.advanceTimersByTime(20));
    const thumb = screen.getByTestId("thumb");
    fireEvent(thumb, new MouseEvent("pointerdown", { bubbles: true, clientX: 100 }));
    fireEvent(window, new MouseEvent("pointermove", { bubbles: true, clientX: 400 }));
    const desiredScroll = viewport.scrollLeft;
    expect(desiredScroll).toBeGreaterThan(1500);
    fireEvent.scroll(viewport);
    act(() => vi.advanceTimersByTime(20));
    const range = JSON.parse(screen.getByTestId("range").textContent!);
    expect(range.startTime).toBeGreaterThan(0);
    act(() => vi.advanceTimersByTime(130));
    expect(viewport.scrollLeft).toBeCloseTo(desiredScroll);
    fireEvent(window, new MouseEvent("pointerup", { bubbles: true }));
  });

  it("preserves a scrollbar track click made while wheel zoom is pending", () => {
    render(<Harness time={0} />);
    const viewport = screen.getByTestId("viewport");
    fireEvent.wheel(viewport, { deltaY: -1500, clientX: 500 });
    act(() => vi.advanceTimersByTime(20));
    const track = screen.getByTestId("track");
    vi.spyOn(track, "getBoundingClientRect").mockReturnValue({ left: 0, width: 1000 } as DOMRect);
    fireEvent(track, new MouseEvent("pointerdown", { bubbles: true, clientX: 700 }));
    const desiredScroll = viewport.scrollLeft;
    expect(desiredScroll).toBeGreaterThan(2000);
    act(() => vi.advanceTimersByTime(150));
    expect(viewport.scrollLeft).toBeCloseTo(desiredScroll);
    fireEvent(window, new MouseEvent("pointerup", { bubbles: true }));
  });
});
