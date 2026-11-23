import { fireEvent, render } from "@testing-library/react";
import { useRef, useState } from "react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { CanvasResizeOverlay } from "@/components/CanvasResizeOverlay";
import type { BackgroundConfig } from "@/types/video";

const OriginalResizeObserver = window.ResizeObserver;
const events: string[] = [];

beforeEach(() => {
  events.length = 0;
  window.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  } as unknown as typeof ResizeObserver;
  // The pointer is released before the next animation frame runs.
  vi.spyOn(window, "requestAnimationFrame").mockImplementation(() => 1);
  vi.spyOn(window, "cancelAnimationFrame").mockImplementation(() => {});
});

afterEach(() => {
  window.ResizeObserver = OriginalResizeObserver;
  vi.restoreAllMocks();
});

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
            ({ width: 800, height: 450, x: 0, y: 0, left: 0, top: 0, right: 800, bottom: 450, toJSON() {} }) as DOMRect;
        }
      }}
    >
      <CanvasResizeOverlay
        previewContainerRef={containerRef}
        backgroundConfig={config}
        setBackgroundConfig={(update) => {
          events.push("resize");
          setConfig(update);
        }}
        beginBatch={() => events.push("beginBatch")}
        commitBatch={() => events.push("commitBatch")}
      />
    </div>
  );
}

it("applies the size under the pointer when it is released before the next frame", () => {
  const { container } = render(<Harness />);
  const east = container.querySelectorAll<HTMLElement>(".canvas-resize-handle")[3];

  fireEvent.pointerDown(east, { pointerId: 7, button: 0, isPrimary: true, clientX: 800, clientY: 225 });
  fireEvent.pointerMove(window, { pointerId: 7, clientX: 850, clientY: 225 });
  fireEvent.pointerUp(window, { pointerId: 7, clientX: 850, clientY: 225 });

  expect(events).toEqual(["beginBatch", "resize", "commitBatch"]);
  expect(container.querySelector(".canvas-resize-label")?.textContent).toContain("1800 x 900");
});
