import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
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

function renderChain() {
  const onCloseProject = vi.fn();
  const { container } = render(
    <SequencePillChain
      composition={composition}
      activeClipId={null}
      spreadFromClipId={null}
      onSelectClip={vi.fn()}
      onInsertClip={vi.fn()}
      onRemoveClip={vi.fn()}
      onModeChange={vi.fn()}
      onCloseProject={onCloseProject}
    />,
  );
  const popover = container.querySelector(".sequence-mode-popover") as HTMLElement;
  return { container, popover, onCloseProject };
}

describe("SequencePillChain keyboard access", () => {
  it("keeps the hidden popover inert so Tab cannot reach its invisible buttons", () => {
    const { popover } = renderChain();
    expect(popover.hasAttribute("inert")).toBe(true);
    expect(popover.getAttribute("aria-hidden")).toBe("true");
  });

  it("opens on focus with the focused clip's actions and closes on Escape", () => {
    const { container, popover } = renderChain();
    const rootPill = container.querySelector(".sequence-pill") as HTMLButtonElement;
    fireEvent.focus(rootPill);

    expect(popover.hasAttribute("inert")).toBe(false);
    expect(popover.getAttribute("aria-hidden")).toBe("false");
    expect(screen.getByRole("button", { name: en.closeProject })).toBeTruthy();

    fireEvent.keyDown(rootPill, { key: "Escape" });
    expect(popover.hasAttribute("inert")).toBe(true);
  });
});
