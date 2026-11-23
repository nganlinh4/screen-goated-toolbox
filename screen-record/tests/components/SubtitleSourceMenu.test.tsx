import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { SubtitleTrack } from "@/components/timeline/SubtitleTrack";
import { NarrationTrack } from "@/components/timeline/NarrationTrack";
import { ZoomBlockLayer } from "@/components/timeline/ZoomBlockLayer";
import { clampMenuPosition } from "@/components/timeline/SubtitleSourceMenu";
import { defaultSubtitleStyle } from "@/lib/subtitleDefaults";
import ko from "@/i18n/ko";
import type { NarrationSegment, SubtitleSegment, VideoSegment } from "@/types/video";

vi.mock("@/hooks/useSettings", () => ({
  useSettings: () => ({ t: ko, lang: "ko" }),
}));

const subtitle: SubtitleSegment = {
  id: "s1",
  startTime: 1,
  endTime: 3,
  text: "Hello",
  style: defaultSubtitleStyle(),
};

const segment = {
  trimStart: 0,
  trimEnd: 10,
  zoomKeyframes: [],
  textSegments: [],
  subtitleSegments: [subtitle],
  subtitleTracks: [{ id: "subtitle-track-original", kind: "original", segments: [subtitle] }],
  activeSubtitleView: { kind: "track", trackId: "subtitle-track-original" },
} as unknown as VideoSegment;

const originalRect = HTMLElement.prototype.getBoundingClientRect;
afterEach(() => {
  HTMLElement.prototype.getBoundingClientRect = originalRect;
});

function openMenuAt(clientX: number, clientY: number) {
  const onAssign = vi.fn();
  render(
    <SubtitleTrack
      segment={segment}
      duration={10}
      editingSubtitleId={null}
      onSubtitleClick={() => {}}
      onHandleDragStart={() => {}}
      isDeviceAudioAvailable
      isMicAudioAvailable
      onAssignSubtitleSourceGroup={onAssign}
    />,
  );
  HTMLElement.prototype.getBoundingClientRect = function getRect(this: HTMLElement) {
    if (this.classList.contains("subtitle-source-menu")) {
      return { x: 0, y: 0, left: 0, top: 0, width: 200, height: 120, right: 200, bottom: 120, toJSON() {} } as DOMRect;
    }
    return originalRect.call(this);
  };
  const block = document.querySelector<HTMLElement>('[data-subtitle-id="s1"]') ?? screen.getByText("Hello");
  fireEvent.contextMenu(block, { clientX, clientY });
  return { onAssign, menu: () => document.querySelector<HTMLElement>(".subtitle-source-menu") };
}

describe("subtitle source menu", () => {
  it("clamps into the viewport, focuses the first item and uses translated labels", () => {
    const { menu } = openMenuAt(window.innerWidth - 10, window.innerHeight - 5);
    const element = menu()!;
    expect(element).not.toBeNull();
    expect(parseFloat(element.style.left)).toBeLessThanOrEqual(window.innerWidth - 200 - 8);
    expect(parseFloat(element.style.top)).toBeLessThanOrEqual(window.innerHeight - 120 - 8);
    const items = screen.getAllByRole("menuitem");
    expect(items.map((item) => item.textContent)).toEqual([
      ko.subtitleSourceUnassigned,
      ko.subtitleSourceDeviceAudio,
      ko.subtitleSourceMicrophone,
    ]);
    expect(document.activeElement).toBe(items[0]);
    fireEvent.keyDown(element, { key: "ArrowDown" });
    expect(document.activeElement).toBe(items[1]);
  });

  it("closes on Escape and on a pointer press outside it", () => {
    const { menu } = openMenuAt(100, 100);
    act(() => {
      window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    });
    expect(menu()).toBeNull();

    const block = document.querySelector<HTMLElement>('[data-subtitle-id="s1"]') ?? screen.getByText("Hello");
    fireEvent.contextMenu(block, { clientX: 100, clientY: 100 });
    expect(menu()).not.toBeNull();
    act(() => {
      document.body.dispatchEvent(new Event("pointerdown", { bubbles: true }));
    });
    expect(menu()).toBeNull();
  });

  it("keeps the clamp pure for any viewport", () => {
    expect(clampMenuPosition(990, 790, 200, 120, 1000, 800)).toEqual({ left: 792, top: 672 });
    expect(clampMenuPosition(-5, -5, 200, 120, 1000, 800)).toEqual({ left: 8, top: 8 });
  });

  it("translates zoom transition and narration boundary labels", () => {
    const narration: NarrationSegment = {
      id: "n1", rawAudioPath: "C:/n1.wav", name: "N1", duration: 2, startTime: 0,
      inPoint: 0, outPoint: 2, addedAt: 0, narrationAlignmentMode: "estimated",
    } as unknown as NarrationSegment;
    render(
      <>
        <ZoomBlockLayer
          blocks={[
            { id: "z1", startTime: 0, endTime: 2, zoomFactor: 2, positionX: 0.5, positionY: 0.5, enabled: true },
            { id: "z2", startTime: 3, endTime: 5, zoomFactor: 2, positionX: 0.5, positionY: 0.5, enabled: true },
          ] as never}
          duration={10}
          editingKeyframeId={0}
          hoveredBlockIdx={null}
          globalDragVisualMode={null as never}
          trackWidth={1000}
          trackRef={{ current: null }}
          onKeyframeClick={vi.fn()}
          onKeyframeDragStart={vi.fn()}
          onHoverBlock={vi.fn()}
          startResizeBlock={vi.fn()}
          startResizeTransition={vi.fn()}
          onToggleDirectTransition={vi.fn()}
        />
        <NarrationTrack segments={[narration]} duration={10} canvasWidthPx={1000} />
      </>,
    );
    expect(screen.getByRole("button", { name: ko.zoomTransitionLink })).toBeInTheDocument();
    expect(screen.getAllByTitle(ko.zoomEaseIn).length).toBeGreaterThan(0);
    expect(screen.getByTitle(ko.narrationEstimatedBoundary)).toHaveTextContent(ko.narrationEstimatedBadge);
    expect(document.body.textContent).not.toMatch(/Ease in|Transition directly|Estimated boundary/);
  });
});
