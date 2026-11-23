import { act, fireEvent, render } from "@testing-library/react";
import { useRef, useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { CanvasResizeOverlay } from "@/components/CanvasResizeOverlay";
import type { BackgroundConfig } from "@/types/video";

const resizeCallbacks: Array<() => void> = [];
const OriginalResizeObserver = window.ResizeObserver;
let containerRect = { width: 800, height: 450 };

beforeEach(() => {
  resizeCallbacks.length = 0;
  containerRect = { width: 800, height: 450 };
  window.ResizeObserver = class {
    constructor(callback: ResizeObserverCallback) {
      resizeCallbacks.push(() => callback([], this as unknown as ResizeObserver));
    }
    observe() {}
    unobserve() {}
    disconnect() {}
  } as unknown as typeof ResizeObserver;
  vi.spyOn(window, "requestAnimationFrame").mockImplementation((callback) => {
    callback(0);
    return 1;
  });
});

afterEach(() => {
  window.ResizeObserver = OriginalResizeObserver;
});

const setConfigSpy = vi.fn();

function Harness() {
  const containerRef = useRef<HTMLDivElement>(null);
  const [config, setConfig] = useState({
    canvasMode: "custom",
    canvasWidth: 1600,
    canvasHeight: 900,
  } as unknown as BackgroundConfig);
  return (
    <div
      ref={(node) => {
        containerRef.current = node;
        if (node) {
          node.getBoundingClientRect = () =>
            ({ ...containerRect, x: 0, y: 0, left: 0, top: 0, right: containerRect.width, bottom: containerRect.height, toJSON() {} }) as DOMRect;
        }
      }}
    >
      <CanvasResizeOverlay
        previewContainerRef={containerRef}
        backgroundConfig={config}
        setBackgroundConfig={(update) => {
          setConfigSpy(update);
          setConfig(update);
        }}
        beginBatch={vi.fn()}
        commitBatch={vi.fn()}
      />
    </div>
  );
}

function handle(container: HTMLElement, index: number) {
  return container.querySelectorAll<HTMLElement>(".canvas-resize-handle")[index];
}

describe("CanvasResizeOverlay", () => {
  it("lays out handles from measured geometry and re-lays them out on container resize", () => {
    const { container } = render(<Harness />);
    // Handle order: n, s, w, e, nw, ne, sw, se. Canvas 1600x900 fits 800x450 at scale 0.5.
    expect(handle(container, 3).style.left).toBe("800px");
    expect(handle(container, 1).style.top).toBe("450px");

    containerRect = { width: 400, height: 225 };
    act(() => resizeCallbacks.forEach((fire) => fire()));
    expect(handle(container, 3).style.left).toBe("400px");
    expect(handle(container, 1).style.top).toBe("225px");
  });

  it("resizes with primary pointer drags and ignores secondary buttons", () => {
    const { container } = render(<Harness />);
    const east = handle(container, 3);

    fireEvent.pointerDown(east, { pointerId: 7, button: 2, isPrimary: true, clientX: 800, clientY: 225 });
    fireEvent.pointerMove(window, { pointerId: 7, clientX: 850, clientY: 225 });
    expect(setConfigSpy).not.toHaveBeenCalled();

    fireEvent.pointerDown(east, { pointerId: 7, button: 0, isPrimary: true, clientX: 800, clientY: 225 });
    fireEvent.pointerMove(window, { pointerId: 99, clientX: 900, clientY: 225 });
    expect(setConfigSpy).not.toHaveBeenCalled();
    fireEvent.pointerMove(window, { pointerId: 7, clientX: 850, clientY: 225 });
    expect(setConfigSpy).toHaveBeenCalledTimes(1);
    // 50 screen px at scale 0.5, both sides grow: +200 canvas px.
    expect(container.querySelector(".canvas-resize-label")?.textContent).toContain("1800 x 900");

    fireEvent.pointerUp(window, { pointerId: 7, clientX: 850, clientY: 225 });
    fireEvent.pointerMove(window, { pointerId: 7, clientX: 950, clientY: 225 });
    expect(setConfigSpy).toHaveBeenCalledTimes(1);
  });
});
