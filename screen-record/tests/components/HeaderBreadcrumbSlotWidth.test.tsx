import { render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { HeaderBreadcrumbSlot } from "@/components/header/HeaderBreadcrumbSlot";
import { SequencePillChain } from "@/components/SequencePillChain";
import en from "@/i18n/en";
import type { ProjectComposition, ProjectCompositionClip } from "@/types/video";

vi.mock("@/hooks/useSettings", () => ({
  useSettings: () => ({ t: en, lang: "en" }),
}));

function clip(id: string, role: "root" | "snapshot"): ProjectCompositionClip {
  return {
    id,
    role,
    name: id,
    duration: 5,
    segment: { trimStart: 0, trimEnd: 5, trimSegments: [] },
    backgroundConfig: { scale: 100, borderRadius: 0, backgroundType: "solid" },
    mousePositions: [],
  } as unknown as ProjectCompositionClip;
}

const composition = {
  mode: "separate",
  selectedClipId: "root",
  focusedClipId: "root",
  clips: [clip("root", "root"), clip("second", "snapshot")],
} as unknown as ProjectComposition;

const observers: Array<() => void> = [];
const OriginalResizeObserver = window.ResizeObserver;

afterEach(() => {
  observers.length = 0;
  window.ResizeObserver = OriginalResizeObserver;
});

describe("Header breadcrumb slot width", () => {
  it("sets the measured pill-chain width on the slot that reads it", () => {
    window.ResizeObserver = class {
      constructor(callback: ResizeObserverCallback) {
        observers.push(() => callback([], this as unknown as ResizeObserver));
      }
      observe() {}
      unobserve() {}
      disconnect() {}
    } as unknown as typeof ResizeObserver;
    const scrollWidth = vi.spyOn(HTMLElement.prototype, "scrollWidth", "get").mockReturnValue(512);

    const { container } = render(
      <HeaderBreadcrumbSlot hidden={false}>
        <SequencePillChain
          composition={composition}
          activeClipId={null}
          spreadFromClipId={null}
          onSelectClip={vi.fn()}
          onInsertClip={vi.fn()}
          onRemoveClip={vi.fn()}
          onModeChange={vi.fn()}
        />
      </HeaderBreadcrumbSlot>,
    );
    observers.forEach((fire) => fire());

    const slot = container.querySelector<HTMLElement>(".header-breadcrumb-slot")!;
    const chain = container.querySelector<HTMLElement>(".sequence-focus-breadcrumb")!;
    expect(slot.style.getPropertyValue("--breadcrumb-content-w")).toBe("512px");
    expect(slot.style.width).toBe("min(var(--breadcrumb-content-w, 360px), 70%)");
    expect(chain.style.getPropertyValue("--breadcrumb-content-w")).toBe("");
    scrollWidth.mockRestore();
  });
});
