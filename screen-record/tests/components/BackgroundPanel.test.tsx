import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useState, type SetStateAction } from "react";
import { vi } from "vitest";

import downloadableBackgrounds from "@/config/downloadable-backgrounds.json";
import { BackgroundPanel } from "@/components/sidepanel/BackgroundPanel";
import { invoke } from "@/lib/ipc";
import { DEFAULT_BACKGROUND_CONFIG } from "@/lib/appUtils";
import { DEFAULT_WEBCAM_CONFIG } from "@/lib/webcam";
import { useEditorHistory } from "@/hooks/useEditorHistory";
import type { BackgroundConfig } from "@/types/video";

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
        beginBatch={vi.fn()}
        commitBatch={vi.fn()}
      />,
    );

    expect(screen.getByText("Background Style")).toBeInTheDocument();
    const activePreset = container.querySelector(`.bg-preset-${DEFAULT_BACKGROUND_CONFIG.backgroundType}`);
    expect(activePreset).toHaveAttribute("aria-pressed", "true");
    expect(container.querySelectorAll('[class*="bg-preset-"][aria-pressed="false"]').length).toBeGreaterThan(0);
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

  it("records one undo step per slider drag and per held-key adjustment", async () => {
    let history!: ReturnType<typeof useEditorHistory>;
    function Harness() {
      const [config, setConfig] = useState<BackgroundConfig>(DEFAULT_BACKGROUND_CONFIG);
      history = useEditorHistory({
        initialSnapshot: {
          segment: null, composition: null, backgroundConfig: config, webcamConfig: DEFAULT_WEBCAM_CONFIG,
          duration: 1, currentRecordingMode: "withoutCursor", currentRawVideoPath: "",
          currentRawMicAudioPath: "", currentRawWebcamVideoPath: "",
        },
        applySnapshot: (snapshot) => setConfig(snapshot.backgroundConfig),
      });
      const setBackgroundConfig = (value: SetStateAction<BackgroundConfig>) => {
        const next = typeof value === "function" ? value(history.getSnapshot().backgroundConfig) : value;
        history.setBackgroundConfig(next);
        setConfig(next);
      };
      return (
        <BackgroundPanel backgroundConfig={config} setBackgroundConfig={setBackgroundConfig}
          recentUploads={[]} onRemoveRecentUpload={vi.fn()} onBackgroundUpload={vi.fn()}
          isBackgroundUploadProcessing={false}
          beginBatch={history.beginBatch} commitBatch={history.commitBatch} />
      );
    }
    const { container } = render(<Harness />);
    const scale = container.querySelector(".video-size-field input[type=range]") as HTMLInputElement;
    const startScale = DEFAULT_BACKGROUND_CONFIG.scale;

    fireEvent.pointerDown(scale, { button: 0, pointerId: 1 });
    for (const value of [startScale - 1, startScale - 5, startScale - 10, startScale - 20]) {
      fireEvent.change(scale, { target: { value: String(value) } });
    }
    fireEvent.pointerUp(scale, { button: 0, pointerId: 1 });

    fireEvent.keyDown(scale, { key: "ArrowRight" });
    fireEvent.change(scale, { target: { value: String(startScale - 19) } });
    fireEvent.keyDown(scale, { key: "ArrowRight" });
    fireEvent.change(scale, { target: { value: String(startScale - 18) } });
    fireEvent.keyUp(scale, { key: "ArrowRight" });

    act(() => history.undo());
    expect(scale.value).toBe(String(startScale - 20));
    act(() => history.undo());
    expect(scale.value).toBe(String(startScale));
    expect(history.canUndo).toBe(false);
    await waitFor(() => expect(invoke).toHaveBeenCalled());
  });
});
