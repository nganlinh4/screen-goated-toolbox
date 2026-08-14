import { render, screen, waitFor } from "@testing-library/react";
import { vi } from "vitest";

import downloadableBackgrounds from "@/config/downloadable-backgrounds.json";
import { BackgroundPanel } from "@/components/sidepanel/BackgroundPanel";
import { invoke } from "@/lib/ipc";
import { DEFAULT_BACKGROUND_CONFIG } from "@/lib/appUtils";

vi.mock("@/lib/ipc", () => ({
  invoke: vi.fn().mockResolvedValue({}),
}));

vi.mock("@/lib/renderer/builtInBackgrounds", () => ({
  BUILT_IN_BACKGROUND_SWATCHES: new Proxy({}, {
    get: () => ({ backgroundColor: "#000" }),
  }),
}));

describe("BackgroundPanel", () => {
  it("keeps background choices while omitting the redundant recent-upload limit control", async () => {
    const { container } = render(
      <BackgroundPanel
        backgroundConfig={DEFAULT_BACKGROUND_CONFIG}
        setBackgroundConfig={vi.fn()}
        recentUploads={["https://example.test/uploaded-background.jpg"]}
        onRemoveRecentUpload={vi.fn()}
        onBackgroundUpload={vi.fn()}
        isBackgroundUploadProcessing={false}
      />,
    );

    expect(screen.getByText("Background Style")).toBeInTheDocument();
    expect(screen.queryByText("Recent uploads")).not.toBeInTheDocument();
    expect(container.querySelector('input[type="range"][max="24"]')).not.toBeInTheDocument();
    expect(screen.getByRole("img", { name: "Upload 1" })).toBeInTheDocument();
    expect(
      screen.getByRole("img", { name: downloadableBackgrounds[0].id }),
    ).toBeInTheDocument();

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("get_bg_download_states", {
        ids: downloadableBackgrounds.map((background) => background.id),
      });
    });
  });
});
