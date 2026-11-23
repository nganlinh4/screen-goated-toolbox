import { fireEvent, render, screen, within } from "@testing-library/react";
import { vi } from "vitest";

import { BackgroundPanel } from "@/components/sidepanel/BackgroundPanel";
import { ZoomPanel } from "@/components/sidepanel/ZoomPanel";
import { DEFAULT_BACKGROUND_CONFIG } from "@/lib/appUtils";
import {
  BUILT_IN_BACKGROUND_NAME_KEYS,
  BUILT_IN_BACKGROUND_PANEL_ORDER,
} from "@/lib/backgroundPresets";
import en from "@/i18n/en";
import ko from "@/i18n/ko";
import vi_ from "@/i18n/vi";
import type { VideoSegment } from "@/types/video";

vi.mock("@/lib/ipc", () => ({
  invoke: vi.fn().mockResolvedValue({}),
}));

vi.mock("@/lib/renderer/builtInBackgrounds", () => ({
  BUILT_IN_BACKGROUND_SWATCHES: new Proxy({}, {
    get: () => ({ backgroundColor: "#000" }),
  }),
}));

const UPLOAD = "https://example.test/uploaded-background.jpg";

describe("background swatch and zoom delete controls", () => {
  it("renders the recent-upload delete badge as a named sibling button, not inside the swatch", () => {
    const onRemove = vi.fn();
    const setBackgroundConfig = vi.fn();
    render(
      <BackgroundPanel
        backgroundConfig={{ ...DEFAULT_BACKGROUND_CONFIG, backgroundType: "custom", customBackground: UPLOAD }}
        setBackgroundConfig={setBackgroundConfig}
        recentUploads={[UPLOAD]}
        onRemoveRecentUpload={onRemove}
        onBackgroundUpload={vi.fn()}
        isBackgroundUploadProcessing={false}
        beginBatch={vi.fn()}
        commitBatch={vi.fn()}
      />,
    );

    const remove = screen.getByRole("button", { name: "Remove uploaded background" });
    const swatch = screen.getByRole("img", { name: "Upload 1" }).closest("button");
    expect(swatch).not.toBeNull();
    expect(swatch).toHaveAttribute("aria-pressed", "true");
    expect(swatch!.contains(remove)).toBe(false);
    expect(within(swatch!).queryByRole("button")).toBeNull();

    remove.focus();
    expect(remove).toHaveFocus();
    fireEvent.click(remove);
    expect(onRemove).toHaveBeenCalledWith(UPLOAD);
    expect(setBackgroundConfig).not.toHaveBeenCalled();
  });

  it("gives every built-in background a distinct localized name and pressed state", () => {
    render(
      <BackgroundPanel
        backgroundConfig={{ ...DEFAULT_BACKGROUND_CONFIG, backgroundType: "gradient1" }}
        setBackgroundConfig={vi.fn()}
        recentUploads={[]}
        onRemoveRecentUpload={vi.fn()}
        onBackgroundUpload={vi.fn()}
        isBackgroundUploadProcessing={false}
        beginBatch={vi.fn()}
        commitBatch={vi.fn()}
      />,
    );

    const names = BUILT_IN_BACKGROUND_PANEL_ORDER.map((id) => en[BUILT_IN_BACKGROUND_NAME_KEYS[id]]);
    expect(new Set(names).size).toBe(names.length);
    for (const id of BUILT_IN_BACKGROUND_PANEL_ORDER) {
      const button = screen.getByRole("button", { name: en[BUILT_IN_BACKGROUND_NAME_KEYS[id]] });
      expect(button).toHaveAttribute("aria-pressed", String(id === "gradient1"));
    }
    for (const locale of [ko, vi_]) {
      for (const id of BUILT_IN_BACKGROUND_PANEL_ORDER) {
        expect(locale[BUILT_IN_BACKGROUND_NAME_KEYS[id]]).not.toBe("");
      }
    }
  });

  it("names the icon-only zoom delete button", () => {
    const onDelete = vi.fn();
    const segment = {
      trimStart: 0,
      trimEnd: 4,
      zoomKeyframes: [],
      textSegments: [],
      zoomBlocks: [{
        id: "z1", startTime: 0, endTime: 2, easeIn: 0.3, easeOut: 0.3,
        zoomFactor: 2, positionX: 0.5, positionY: 0.5, enabled: true,
      }],
    } as unknown as VideoSegment;
    render(
      <ZoomPanel
        segment={segment}
        editingKeyframeId={0}
        zoomFactor={2}
        setZoomFactor={vi.fn()}
        onDeleteKeyframe={onDelete}
        onUpdateZoom={vi.fn()}
        beginBatch={vi.fn()}
        commitBatch={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Delete zoom" }));
    expect(onDelete).toHaveBeenCalledTimes(1);
  });
});
