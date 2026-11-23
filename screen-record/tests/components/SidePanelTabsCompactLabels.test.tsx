import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { SidePanelTabs } from "@/components/sidepanel/SidePanelTabs";
import ko from "@/i18n/ko";

vi.mock("@/hooks/useSettings", () => ({
  useSettings: () => ({ t: ko, lang: "ko" }),
}));

describe("SidePanelTabs compact labels", () => {
  it("uses the locale's short labels instead of English abbreviations", () => {
    render(
      <SidePanelTabs
        activePanel="zoom"
        onPanelChange={vi.fn()}
        hiddenTabs={new Set(["camera", "text"])}
      />,
    );
    expect(screen.getByText(ko.tabBackgroundShort)).toBeInTheDocument();
    expect(screen.getByText(ko.tabNarrationShort)).toBeInTheDocument();
    expect(screen.getByText(ko.tabSubtitlesShort)).toBeInTheDocument();
    expect(screen.queryByText("Bg")).not.toBeInTheDocument();
    expect(screen.queryByText("Subs")).not.toBeInTheDocument();
  });
});
