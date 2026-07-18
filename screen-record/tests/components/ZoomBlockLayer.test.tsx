import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ZoomBlockLayer } from "@/components/timeline/ZoomBlockLayer";
import type { ZoomBlock } from "@/types/video";

const block = (id: string, startTime: number, endTime: number): ZoomBlock => ({
  id,
  startTime,
  endTime,
  easeIn: 0.4,
  easeOut: 0.4,
  zoomFactor: 1.5,
  positionX: 0.5,
  positionY: 0.5,
  enabled: true,
});

describe("ZoomBlockLayer direct transitions", () => {
  it("makes the full gap linkable and exposes persistent linked state", () => {
    const onToggleDirectTransition = vi.fn();
    const blocks = [block("a", 1, 3), block("b", 6, 8)];
    const props = {
      blocks,
      duration: 10,
      editingKeyframeId: null,
      hoveredBlockIdx: null,
      globalDragVisualMode: null,
      trackWidth: 1000,
      trackRef: { current: null },
      onKeyframeClick: vi.fn(),
      onKeyframeDragStart: vi.fn(),
      onHoverBlock: vi.fn(),
      startResizeBlock: vi.fn(),
      startResizeTransition: vi.fn(),
      onToggleDirectTransition,
    } as const;

    const { rerender } = render(<ZoomBlockLayer {...props} />);
    const bridge = document.querySelector(".zoom-direct-transition") as HTMLElement;
    expect(bridge.style.left).toBe("30%");
    expect(bridge.style.width).toBe("30%");

    fireEvent.click(screen.getByRole("button", { name: "Link manual zoom transition" }));
    expect(onToggleDirectTransition).toHaveBeenCalledWith(0);

    rerender(
      <ZoomBlockLayer
        {...props}
        blocks={[{ ...blocks[0], directTransitionToNext: true }, blocks[1]]}
      />,
    );
    expect(screen.getByRole("button", { name: "Unlink manual zoom transition" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
  });
});
